use std::{
    cell::OnceCell,
    io::{IoSlice, IoSliceMut},
};

use anyhow::{anyhow, Result};
use nix::{sys, unistd};
use syscalls::Sysno;

use crate::{
    proctool::terminals,
    teleclient::myprocfs::{self, MemoryMap},
};

pub struct ProcessController {
    pid: unistd::Pid,
    memory_maps: OnceCell<Vec<MemoryMap>>,
}

impl ProcessController {
    pub fn new(pid: unistd::Pid) -> Self {
        Self {
            pid,
            memory_maps: OnceCell::new(),
        }
    }

    pub fn attach(&self) -> Result<()> {
        sys::ptrace::attach(self.pid).map_err(|e| anyhow!("PTRACE_ATTACH failed: {}", e))?;
        sys::wait::waitpid(self.pid, Some(sys::wait::WaitPidFlag::WSTOPPED))
            .map_err(|e| anyhow!("failed to waitpid after PTRACE_ATTACH: {}", e))?;
        Ok(())
    }

    pub fn cancel_pending_read(&self) -> Result<()> {
        log::info!("cancel pending read");
        if let Some((sysno, arg)) = self.current_syscall()? {
            log::info!("cancel pending read: syscall {}", sysno);
            // stdin is represented by 0
            if sysno == Sysno::read.id() as u64 && arg == 0 {
                log::info!("cancel pending read: writing to stdin");
                terminals::write_to_stdin(self.pid, "")?;
                self.step_and_wait()?;
            }
            Ok(())
        } else {
            log::info!("cancel pending read: not in a syscall");
            // not in a syscall
            Ok(())
        }
    }

    /// returns (sysno, first arg)
    pub fn current_syscall(&self) -> Result<Option<(u64, u64)>> {
        let registers = self.get_registers()?;
        let data = sys::ptrace::read(self.pid, registers.pc as *mut libc::c_void)?;
        let current_instruction = (data & 0xffffffff) as u64;
        if current_instruction as u32 == SVC {
            Ok(Some((registers.regs[8], registers.regs[0])))
        } else {
            Ok(None)
        }
    }

    pub fn ensure_not_in_syscall(&self) -> Result<()> {
        // TODO: this method is flawed
        //   if we are in a normal syscall then single-stepping is fine
        //   if we are in a nanosleep we just need to wait that amount of time
        //   if we are in a read we're probably reading from stdin (otherwise would not have blocked)
        //     we can send a line to the process's stdin
        //
        // as currently written the method only works for nanosleep because it just spins until the syscall
        // returns; it spins forever for reading from stdin
        //
        // alternatively, seems like we could single-step; if that fails to advance PC, then do PTRACE_SYSCALL
        // to wait for syscall exit
        let initial_registers = self.get_registers()?;
        let initial_pc = initial_registers.pc;

        loop {
            self.step_and_wait()?;
            let current_registers = self.get_registers()?;
            if current_registers.pc != initial_pc {
                break;
            }
            // TODO: sleep for an interval and have a timeout
            // would also be nice to return to the user that the program may need user interaction
        }

        Ok(())
    }

    pub fn prepare_syscall(&self, sysno: Sysno, args: Vec<i64>) -> Result<()> {
        let new_pc = self.find_svc_instruction()?;
        self.prepare_syscall_at_pc(sysno, args, new_pc)
    }

    pub fn prepare_syscall_at_pc(&self, sysno: Sysno, args: Vec<i64>, pc: u64) -> Result<()> {
        let mut registers = self.get_registers()?;

        // syscall number in x8, args in x0, x1, x2, x3...
        registers.regs[8] = sysno.id() as u64;
        for i in 0..args.len() {
            registers.regs[i] = args[i] as u64;
        }
        registers.pc = pc;

        self.set_registers(registers)?;
        Ok(())
    }

    pub fn execute_syscall(&self, sysno: Sysno, args: Vec<i64>) -> Result<u64> {
        self.prepare_syscall(sysno, args)?;
        self.ensure_not_in_syscall()?;
        let registers = self.get_registers()?;
        Ok(registers.regs[0])
    }

    pub fn execute_syscall_at_pc(&self, sysno: Sysno, args: Vec<i64>, pc: u64) -> Result<u64> {
        self.prepare_syscall_at_pc(sysno, args, pc)?;
        self.ensure_not_in_syscall()?;
        let registers = self.get_registers()?;
        Ok(registers.regs[0])
    }

    pub fn find_svc_instruction(&self) -> Result<u64> {
        for memory_map in self.get_memory_maps()? {
            // [vdso] section should always have a syscall instruction
            // more robust: look at every executable section
            if memory_map.label == "[vdso]" {
                return find_svc_instruction_in_map(self.pid, &memory_map);
            }
        }

        Err(anyhow!("could not find [vdso] segment in binary"))
    }

    pub fn get_registers(&self) -> Result<libc::user_regs_struct> {
        let registers = sys::ptrace::getregset::<sys::ptrace::regset::NT_PRSTATUS>(self.pid)
            .map_err(|e| anyhow!("PTRACE_GETREGSET failed: {}", e))?;
        Ok(registers)
    }

    pub fn set_registers(&self, registers: libc::user_regs_struct) -> Result<()> {
        sys::ptrace::setregset::<sys::ptrace::regset::NT_PRSTATUS>(self.pid, registers)
            .map_err(|e| anyhow!("PTRACE_SETREGSET failed: {}", e))?;
        Ok(())
    }

    pub fn wait_for_syscall(&self) -> Result<()> {
        sys::ptrace::syscall(self.pid, None)
            .map_err(|e| anyhow!("PTRACE_SYSCALL failed: {}", e))?;
        sys::wait::waitpid(self.pid, Some(sys::wait::WaitPidFlag::WSTOPPED))
            .map_err(|e| anyhow!("failed to waitpid after PTRACE_SYSCALL: {}", e))?;
        Ok(())
    }

    pub fn stop_at_next_syscall(&self) -> Result<()> {
        sys::ptrace::syscall(self.pid, None)
            .map_err(|e| anyhow!("PTRACE_SYSCALL failed: {}", e))?;
        self.wait_for_anything()?;
        Ok(())
    }

    pub fn continue_syscall(&self) -> Result<()> {
        sys::ptrace::syscall(self.pid, None).map_err(|e| anyhow!("PTRACE_SYSCALL: {}", e))?;
        self.wait_for_anything()?;
        Ok(())
    }

    pub fn wait_for_anything(&self) -> Result<()> {
        // per "Stopped states" section of ptrace(2)
        sys::wait::waitpid(self.pid, Some(sys::wait::WaitPidFlag::__WALL))
            .map_err(|e| anyhow!("waitpid: {}", e))?;
        Ok(())
    }

    pub fn is_writing_to_stdout(&self) -> Result<bool> {
        let registers = self.get_registers()?;
        Ok(registers.regs[8] == Sysno::write.id() as u64
            && registers.regs[0] == libc::STDOUT_FILENO as u64)
    }

    pub fn map_svc_region(&self) -> Result<u64> {
        let mut highest_addr: u64 = 0;
        for memory_map in self.get_memory_maps()? {
            highest_addr = std::cmp::max(memory_map.base_address + memory_map.size, highest_addr);
        }

        let addr = highest_addr + 4096;
        let region_size = 4096;
        println!("trying to map to addr {:#x}", addr);
        let r = self.execute_syscall(
            Sysno::mmap,
            vec![
                addr as i64,
                region_size,
                (libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC) as i64,
                (libc::MAP_PRIVATE | libc::MAP_ANONYMOUS) as i64,
                -1,
                0,
            ],
        )?;
        if r as *mut libc::c_void == libc::MAP_FAILED {
            return Err(anyhow!("mmap failed at {:#x} (size={})", addr, region_size));
        }

        println!("mmap returned {:#x}", r);

        let mut bytes = Vec::new();
        for _ in 0..(region_size as usize) / SVC_BYTES.len() {
            bytes.extend_from_slice(&SVC_BYTES[..]);
        }

        let local_iov = IoSlice::new(&bytes);
        let remote_iov = sys::uio::RemoteIoVec {
            base: addr as usize,
            len: bytes.len(),
        };
        let nwritten = sys::uio::process_vm_writev(self.pid, &[local_iov], &[remote_iov])
            .map_err(|e| anyhow!("process_vm_writev failed: {}", e))?;
        if nwritten == 0 {
            return Err(anyhow!("failed to write data"));
        }

        Ok(addr)
    }

    fn get_segment_address(&self, label: &str) -> Result<(u64, u64)> {
        for map in self.get_memory_maps()? {
            if map.label == label {
                return Ok((map.base_address, map.size));
            }
        }

        Err(anyhow!("could not find [vvar] address"))
    }

    pub fn unmap_existing_regions(&self, svc_region_addr: u64) -> Result<()> {
        let (vvar_address, _) = self.get_segment_address("[vvar]")?;
        let (vdso_address, vdso_size) = self.get_segment_address("[vdso]")?;

        println!("align: {}", (vdso_address + vdso_size) % 4096);
        match self.execute_syscall_at_pc(
            Sysno::munmap,
            vec![(vdso_address + vdso_size) as i64, svc_region_addr as i64],
            svc_region_addr,
        ) {
            Ok(r) => println!("munmap returned (1): {:#x} ({})", r, r as i64),
            Err(e) => println!("munmap error: {}", e),
        }

        match self.execute_syscall_at_pc(
            Sysno::munmap,
            vec![0, vvar_address as i64],
            svc_region_addr,
        ) {
            Ok(r) => println!("munmap returned (2): {:#x} ({})", r, r as i64),
            Err(e) => println!("munmap error: {}", e),
        }

        // for (i, memory_map) in self.get_memory_maps()?.iter().enumerate() {
        //     if i == 15
        //         || i == 16
        //         || i == 17
        //         || i == 18
        //         || i == 19
        //         || i == 20
        //         || i == 21
        //         || i == 22
        //         || !memory_map.readable && !memory_map.writable && !memory_map.executable
        //     {
        //         println!("skipping {:#x}", memory_map.base_address);
        //         continue;
        //     }

        //     println!("unmapping {:#x} (i={})", memory_map.base_address, i);
        //     match self.execute_syscall_at_pc(
        //         Sysno::munmap,
        //         vec![memory_map.base_address as i64, memory_map.size as i64],
        //         svc_region_addr,
        //     ) {
        //         Ok(r) => println!("return value: {}", r),
        //         Err(e) => println!("munmap error: {}", e),
        //     }
        // }
        Ok(())
    }

    pub fn map_and_fill_region(
        &self,
        svc_region_addr: u64,
        memory_map: &myprocfs::MemoryMap,
    ) -> Result<()> {
        self.execute_syscall_at_pc(
            Sysno::munmap,
            vec![memory_map.base_address as i64, memory_map.size as i64],
            svc_region_addr,
        )?;

        let mut prot = 0;
        if memory_map.readable {
            prot |= libc::PROT_READ;
        }

        if memory_map.writable {
            prot |= libc::PROT_WRITE;
        }

        if memory_map.executable {
            prot |= libc::PROT_EXEC;
        }

        // TODO: this definitely doesn't handle shared memory correctly
        let options = libc::MAP_ANONYMOUS | libc::MAP_PRIVATE;
        // if memory_map.private {
        //     options |= libc::MAP_PRIVATE;
        // }

        let r = self.execute_syscall_at_pc(
            Sysno::mmap,
            vec![
                memory_map.base_address as i64,
                memory_map.size as i64,
                // we need it to be writable for the next step
                (prot | libc::PROT_WRITE) as i64,
                options as i64,
                -1,
                0,
            ],
            svc_region_addr,
        )?;
        if r as i64 == -1 {
            return Err(anyhow!(
                "mmap failed at {:#x} (size={})",
                memory_map.base_address,
                memory_map.size
            ));
        }

        if memory_map.data.len() > 0 {
            let local_iov = IoSlice::new(&memory_map.data);
            let remote_iov = sys::uio::RemoteIoVec {
                base: memory_map.base_address as usize,
                len: memory_map.data.len(),
            };
            let nwritten = sys::uio::process_vm_writev(self.pid, &[local_iov], &[remote_iov])
                .map_err(|e| anyhow!("process_vm_writev failed: {}", e))?;
            if nwritten == 0 {
                return Err(anyhow!("failed to write data"));
            }
        }

        if !memory_map.writable {
            // if it wasn't supposed to be writable, fix it
            self.execute_syscall_at_pc(
                Sysno::mprotect,
                vec![
                    memory_map.base_address as i64,
                    memory_map.size as i64,
                    prot as i64,
                ],
                svc_region_addr,
            )?;
        }

        Ok(())
    }

    pub fn step_and_wait(&self) -> Result<()> {
        sys::ptrace::step(self.pid, None)
            .map_err(|e| anyhow!("PTRACE_SINGLESTEP failed: {}", e))?;
        sys::wait::waitpid(self.pid, Some(sys::wait::WaitPidFlag::WSTOPPED))
            .map_err(|e| anyhow!("failed to waitpid (syscall injection): {}", e))?;
        Ok(())
    }

    pub fn inject_bytes(&self, bytes: &[u8]) -> Result<u64> {
        let addr = self.execute_syscall(
            Sysno::mmap,
            vec![
                0,
                bytes.len() as i64,
                (libc::PROT_READ | libc::PROT_WRITE) as i64,
                (libc::MAP_ANON | libc::MAP_PRIVATE) as i64,
                -1,
                0,
            ],
        )?;

        let local_iov = IoSlice::new(bytes);
        let remote_iov = sys::uio::RemoteIoVec {
            base: addr as usize,
            len: bytes.len(),
        };
        let nwritten = sys::uio::process_vm_writev(self.pid, &[local_iov], &[remote_iov])?;
        if nwritten == 0 {
            return Err(anyhow!("failed to write data"));
        }

        Ok(addr)
    }

    pub fn inject_u64s(&self, xs: &[u64]) -> Result<u64> {
        let mut bytes = Vec::new();
        for x in xs {
            bytes.extend_from_slice(&x.to_le_bytes());
        }
        self.inject_bytes(&bytes)
    }

    pub fn detach(&self) -> Result<()> {
        self.detach_generic(None)
    }

    pub fn detach_and_stop(&self) -> Result<()> {
        self.detach_generic(Some(sys::signal::Signal::SIGSTOP))
    }

    pub fn waitpid(&self) -> Result<()> {
        sys::wait::waitpid(self.pid, None).map_err(|e| anyhow!("failed to waitpid: {}", e))?;
        Ok(())
    }

    fn detach_generic(&self, signal: Option<sys::signal::Signal>) -> Result<()> {
        sys::ptrace::detach(self.pid, signal)
            .map_err(|e| anyhow!("PTRACE_DETACH failed: {}", e))?;
        Ok(())
    }

    fn get_memory_maps(&self) -> Result<&Vec<MemoryMap>> {
        self.initialize_memory_maps()?;
        Ok(self.memory_maps.get().unwrap())
    }

    fn initialize_memory_maps(&self) -> Result<()> {
        if self.memory_maps.get().is_some() {
            return Ok(());
        }

        let memory_maps = myprocfs::read_memory_maps(self.pid.as_raw())?;
        let _ = self.memory_maps.set(memory_maps);
        Ok(())
    }
}

fn find_svc_instruction_in_map(pid: unistd::Pid, memory_map: &MemoryMap) -> Result<u64> {
    let mut buffer = vec![0; memory_map.size as usize];
    let local_iov = &mut [IoSliceMut::new(&mut buffer[..])];
    let remote_iov = sys::uio::RemoteIoVec {
        base: memory_map.base_address as usize,
        len: memory_map.size as usize,
    };
    let nread = sys::uio::process_vm_readv(pid, local_iov, &[remote_iov])?;
    if nread == 0 {
        return Err(anyhow!("failed to read any data"));
    }

    for i in 0..buffer.len() - 3 {
        if buffer[i] == SVC_BYTES[0]
            && buffer[i + 1] == SVC_BYTES[1]
            && buffer[i + 2] == SVC_BYTES[2]
            && buffer[i + 3] == SVC_BYTES[3]
        {
            return Ok(memory_map.base_address + i as u64);
        }
    }

    Err(anyhow!("could not find svc instruction in segment"))
}

const SVC: u32 = 0xd4000001;
// little-endian representation: 0x01 0x00 0x00 0xd4
const SVC_BYTES: [u8; 4] = [0x01, 0x00, 0x00, 0xd4];
