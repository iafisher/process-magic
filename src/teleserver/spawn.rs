use std::fs::File;
use std::io::{IoSlice, Seek, SeekFrom, Write};
use std::mem::MaybeUninit;

use anyhow::{anyhow, Result};
use libc::{MAP_FAILED, MAP_FIXED, MAP_SHARED, PROT_EXEC, PROT_READ, PROT_WRITE};
use nix::sys::ptrace as nix_ptrace;
use nix::sys::signal::{self, Signal};
use nix::sys::uio::RemoteIoVec;
use nix::sys::wait::WaitPidFlag;
use nix::unistd::{fork, Pid};
use syscalls::Sysno;

use crate::teleclient::procfs::{self, MemoryMap};

pub fn spawn_process(
    gp_register_data: &Vec<u8>,
    fp_register_data: &Vec<u8>,
    memory_maps: &Vec<MemoryMap>,
) -> Result<()> {
    match unsafe { fork() }? {
        nix::unistd::ForkResult::Parent { child } => {
            println!("in parent, child PID is {}", child);
            nix::sys::wait::waitpid(child, Some(WaitPidFlag::WSTOPPED))
                .map_err(|e| anyhow!("failed to waitpid: {}", e))?;

            let result = initialize_process(child, gp_register_data, fp_register_data, memory_maps);

            // signal::kill(child, Signal::SIGKILL)
            //     .map_err(|e| anyhow!("unable to kill child process: {}", e))?;

            // debugging: freezes the child process so we can inspect it with gdb
            nix_ptrace::detach(child, Some(Signal::SIGSTOP))?;
            result?
        }
        nix::unistd::ForkResult::Child => {
            nix_ptrace::traceme().map_err(|e| anyhow!("failed to ptrace child: {}", e))?;
            signal::raise(Signal::SIGSTOP)?;
        }
    }

    Ok(())
}

fn initialize_process(
    pid: Pid,
    gp_register_data: &Vec<u8>,
    fp_register_data: &Vec<u8>,
    memory_maps: &Vec<MemoryMap>,
) -> Result<()> {
    // important to call this before setting registers as it relies on a valid value of PC
    unmap_existing_memory(pid)?;

    set_registers(pid, libc::NT_PRSTATUS, gp_register_data)?;
    // TODO: fpsr on ARM isn't set correctly
    set_registers(pid, libc::NT_PRFPREG, fp_register_data)?;

    for memory_map in memory_maps {
        write_memory_map(pid, memory_map)?;
    }

    Ok(())
}

fn set_registers(pid: Pid, kind: libc::c_int, register_data: &Vec<u8>) -> Result<()> {
    let mut regs = MaybeUninit::<libc::user_regs_struct>::uninit();
    let iov_len = core::mem::size_of_val(&regs);
    if iov_len != register_data.len() {
        return Err(anyhow!(
            "expected length of register data to be {} but it was {}",
            iov_len,
            register_data.len()
        ));
    }

    let iov = libc::iovec {
        iov_base: regs.as_mut_ptr() as *mut libc::c_void,
        iov_len,
    };

    let p = iov.iov_base as *mut u8;
    for i in 0..register_data.len() {
        unsafe {
            *p.offset(i as isize) = register_data[i];
        }
    }

    unsafe {
        syscalls::syscall!(
            Sysno::ptrace,
            libc::PTRACE_SETREGSET,
            pid.as_raw(),
            kind,
            &iov as *const _
        )
    }
    .map_err(|e| anyhow!("PTRACE_SETREGSET failed: {}", e))?;

    Ok(())
}

fn unmap_existing_memory(pid: Pid) -> Result<()> {
    let memory_maps = procfs::read_memory_maps(pid.as_raw())?;

    let process_registers = nix_ptrace::getregset::<nix::sys::ptrace::regset::NT_PRSTATUS>(pid)
        .map_err(|e| anyhow!("PTRACE_GETREGSET failed: {}", e))?;
    let pc = process_registers.pc;

    let mut _code_page_opt: Option<&MemoryMap> = None;
    for memory_map in memory_maps.iter() {
        if memory_map.base_address <= pc && pc < memory_map.base_address + memory_map.size {
            println!("skipping map with PC for now");
            _code_page_opt = Some(memory_map);
            continue;
        }

        // TODO: can't do this
        if memory_map.size == 2097152 {
            continue;
        }

        if let Err(e) = unmap_one(pid, &memory_map) {
            eprintln!("warning: munmap failed: {}", e);
        }
    }

    // if let Some(code_page) = code_page_opt {
    //     // this is expected to fail as it tried to restore the old instruction, which won't work
    //     // because we just unmapped that page.
    //     let _ = unmap_one(pid, &code_page);
    // }

    Ok(())
}

fn unmap_one(pid: Pid, memory_map: &MemoryMap) -> Result<()> {
    let code = make_syscall(
        pid,
        Sysno::munmap,
        vec![memory_map.base_address, memory_map.size],
    )?;

    if code != 0 {
        return Err(anyhow!("syscall to munmap failed",));
    }

    Ok(())
}

fn make_syscall(pid: Pid, sysno: Sysno, args: Vec<u64>) -> Result<u64> {
    let old_registers = nix_ptrace::getregset::<nix_ptrace::regset::NT_PRSTATUS>(pid)
        .map_err(|e| anyhow!("PTRACE_GETREGSET failed: {}", e))?;
    let mut new_registers = old_registers.clone();

    // syscall number in x8, args in x0, x1, x2, x3...
    new_registers.regs[8] = sysno.id() as u64;
    for i in 0..args.len() {
        new_registers.regs[i] = args[i];
    }

    let p = new_registers.pc as *mut libc::c_void;
    let old_data =
        nix_ptrace::read(pid, p).map_err(|e| anyhow!("PTRACE_PEEKDATA failed: {}", e))?;
    // 0xd4000001 = svc #0
    nix_ptrace::write(pid, p, 0xd4000001)
        .map_err(|e| anyhow!("PTRACE_POKEDATA failed (injecting syscall): {}", e))?;
    nix_ptrace::setregset::<nix_ptrace::regset::NT_PRSTATUS>(pid, new_registers).map_err(|e| {
        anyhow!(
            "PTRACE_SETREGSET failed (setting syscall parameters): {}",
            e
        )
    })?;

    nix_ptrace::step(pid, None).map_err(|e| anyhow!("PTRACE_SINGLESTEP failed: {}", e))?;
    nix::sys::wait::waitpid(pid, Some(WaitPidFlag::WSTOPPED))
        .map_err(|e| anyhow!("failed to waitpid: {}", e))?;

    nix_ptrace::write(pid, p, old_data)
        .map_err(|e| anyhow!("PTRACE_POKEDATA failed (restoring old data): {}", e))?;

    // return in x0
    let registers_after = nix_ptrace::getregset::<nix_ptrace::regset::NT_PRSTATUS>(pid)
        .map_err(|e| anyhow!("PTRACE_GETREGSET failed (checking syscall return): {}", e))?;

    // restore the old registers (including the PC, which undoes the single-step before)
    nix_ptrace::setregset::<nix_ptrace::regset::NT_PRSTATUS>(pid, old_registers)
        .map_err(|e| anyhow!("PTRACE_SETREGSET failed (restoring old registers): {}", e))?;

    Ok(registers_after.regs[0])
}

fn write_memory_map(pid: Pid, memory_map: &MemoryMap) -> Result<()> {
    // TODO: the page that contains PC must be mapped already or else our syscall injection doesn't work
    map_page_in_child(pid, memory_map)?;

    let result = nix::sys::uio::process_vm_writev(
        pid,
        &[IoSlice::new(&memory_map.data)],
        &[RemoteIoVec {
            base: memory_map.base_address as usize,
            len: memory_map.size as usize,
        }],
    );

    if result.is_err() {
        println!("process_vm_writev failed");
        // TODO: is this necessary?
        let mut f = File::options()
            .read(true)
            .write(true)
            .open(format!("/proc/{}/mem", pid.as_raw()))?;

        f.seek(SeekFrom::Start(memory_map.base_address))?;
        if let Err(e) = f.write(&memory_map.data) {
            println!("failed again: {}", e);
        }
    }

    Ok(())
}

fn map_page_in_child(pid: Pid, memory_map: &MemoryMap) -> Result<()> {
    let mut prot = 0;
    if memory_map.readable {
        prot |= PROT_READ;
    }

    if memory_map.writable {
        prot |= PROT_WRITE;
    }

    if memory_map.executable {
        prot |= PROT_EXEC;
    }

    let mut flags = 0;
    if memory_map.private {
        flags |= MAP_SHARED;
    }
    flags |= MAP_FIXED;

    let r = make_syscall(
        pid,
        Sysno::mmap,
        vec![
            memory_map.base_address,
            memory_map.size,
            prot as u64,
            flags as u64,
            0,
            0,
        ],
    )?;

    if r as *mut libc::c_void == MAP_FAILED {
        return Err(anyhow!("mmap failed"));
    }

    Ok(())
}
