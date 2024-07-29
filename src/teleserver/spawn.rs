use std::mem::MaybeUninit;

use anyhow::{anyhow, Result};
use nix::sys::ptrace as nix_ptrace;
use nix::sys::signal::{self, Signal};
use nix::sys::wait::WaitPidFlag;
use nix::unistd::{fork, Pid};
use syscalls::Sysno;

use crate::teleclient::procfs::MemoryMap;

pub fn spawn_process(register_data: &Vec<u8>, memory_maps: &Vec<MemoryMap>) -> Result<()> {
    println!("spawn_process");
    match unsafe { fork() }? {
        nix::unistd::ForkResult::Parent { child } => {
            println!("in parent, child PID is {}", child);
            nix::sys::wait::waitpid(child, Some(WaitPidFlag::WSTOPPED))
                .map_err(|e| anyhow!("failed to waitpid: {}", e))?;

            initialize_process(child, register_data, memory_maps)?;

            // TODO: registers: ptrace()
            // TODO: memory: https://docs.rs/nix/0.29.0/nix/sys/uio/fn.process_vm_writev.html
            println!("tracing child");

            // signal::kill(child, Signal::SIGKILL)
            //     .map_err(|e| anyhow!("unable to kill child process: {}", e))?;

            // debugging: freezes the child process so we can inspect it with gdb
            nix_ptrace::detach(child, Some(Signal::SIGSTOP))?;
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
    register_data: &Vec<u8>,
    memory_maps: &Vec<MemoryMap>,
) -> Result<()> {
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
            libc::NT_PRSTATUS,
            &iov as *const _
        )
    }
    .map_err(|e| anyhow!("PTRACE_SETREGSET failed: {}", e))?;

    // TODO: memory

    Ok(())
}
