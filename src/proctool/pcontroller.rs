use std::{cell::OnceCell, io::{IoSlice, IoSliceMut}};

use anyhow::{anyhow, Result};
use nix::{sys, unistd};
use syscalls::Sysno;

use crate::{proctool::terminals, teleclient::myprocfs::{self, MemoryMap}};

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
        if current_instruction == 0xd4000001 {
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

        let mut registers = self.get_registers()?;

        // syscall number in x8, args in x0, x1, x2, x3...
        registers.regs[8] = sysno.id() as u64;
        for i in 0..args.len() {
            registers.regs[i] = args[i] as u64;
        }
        registers.pc = new_pc;

        self.set_registers(registers)?;
        Ok(())
    }

    pub fn execute_syscall(&self, sysno: Sysno, args: Vec<i64>) -> Result<u64> {
        self.prepare_syscall(sysno, args)?;
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

    // value: 0xd4000001
    // little-endian representation: 0x01 0x00 0x00 0xd4

    for i in 0..buffer.len() - 3 {
        if buffer[i] == 0x01
            && buffer[i + 1] == 0x00
            && buffer[i + 2] == 0x00
            && buffer[i + 3] == 0xd4
        {
            return Ok(memory_map.base_address + i as u64);
        }
    }

    Err(anyhow!("could not find svc instruction in segment"))
}
