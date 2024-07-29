use std::fs::File;
use std::io::{Read, Seek};
use std::mem::MaybeUninit;

use anyhow::{anyhow, Result};
use nix::sys::ptrace as nix_ptrace;
use nix::sys::wait::WaitPidFlag;
use nix::unistd::Pid;
use syscalls::Sysno;

use crate::teleclient::procfs::MemoryMap;

use super::procfs;

pub struct Tracer {
    pid: Pid,
}

impl Tracer {
    pub fn seize_and_interrupt(pid_i32: i32) -> Result<Self> {
        let pid = Pid::from_raw(pid_i32);
        nix_ptrace::seize(pid, nix_ptrace::Options::empty())
            .map_err(|e| anyhow!("failed to seize process: {}", e))?;
        nix_ptrace::interrupt(pid).map_err(|e| anyhow!("failed to interrupt process: {}", e))?;
        nix::sys::wait::waitpid(pid, Some(WaitPidFlag::WSTOPPED))
            .map_err(|e| anyhow!("failed to waitpid: {}", e))?;
        Ok(Self { pid })
    }

    pub fn get_general_purpose_registers(&self) -> Result<Vec<u8>> {
        self.get_registers(libc::NT_PRSTATUS)
    }

    pub fn get_floating_point_registers(&self) -> Result<Vec<u8>> {
        self.get_registers(libc::NT_PRFPREG)
    }

    pub fn get_registers(&self, kind: libc::c_int) -> Result<Vec<u8>> {
        // adapted from https://github.com/facebookexperimental/reverie/blob/852e08e75ddcd0ca3f5ea0ded7e60491051ffb76/safeptrace/src/lib.rs#L515

        // `regs` will be initialized by called ptrace(). We need this instead of just `libc::iovec`
        // so the compiler can tell us the size of `libc::user_regs_struct`, which is processor-
        // dependent.
        let mut regs = MaybeUninit::<libc::user_regs_struct>::uninit();
        let mut iov = libc::iovec {
            iov_base: regs.as_mut_ptr() as *mut libc::c_void,
            iov_len: core::mem::size_of_val(&regs),
        };

        // TODO: also need to copy other kinds of registers besides NT_PRSTATUS
        unsafe {
            syscalls::syscall!(
                Sysno::ptrace,
                libc::PTRACE_GETREGSET,
                self.pid.as_raw(),
                kind,
                // `*mut _` lets the compiler figure out the proper type here
                &mut iov as *mut _
            )
        }
        .map_err(|e| anyhow!("PTRACE_GETREGSET failed: {}", e))?;

        let mut r = Vec::new();
        for i in 0..iov.iov_len as isize {
            let p = iov.iov_base as *mut u8;
            let v = unsafe { *p.offset(i) };
            r.push(v);
        }

        Ok(r)
    }

    pub fn read_memory(&self) -> Result<Vec<MemoryMap>> {
        // https://unix.stackexchange.com/questions/6301/how-do-i-read-from-proc-pid-mem-under-linux

        let mut memory_maps = procfs::read_memory_maps(self.pid.as_raw())?;

        let path = format!("/proc/{}/mem", self.pid);
        let mut file = File::open(&path)?;

        for memory_map in memory_maps.iter_mut() {
            // [vvar] is special data used by the vDSO which for reasons unknown cannot be read via procfs
            // further discussion:
            //   - https://lwn.net/Articles/615809/
            //   - https://stackoverflow.com/questions/42730260/
            if !memory_map.readable || memory_map.label == "[vvar]" {
                continue;
            }

            file.seek(std::io::SeekFrom::Start(memory_map.base_address))
                .map_err(|e| {
                    anyhow!(
                        "unable to seek to {:#x} in {}: {}",
                        memory_map.base_address,
                        path,
                        e
                    )
                })?;

            let mut buf = vec![0u8; memory_map.size as usize];
            if let Err(e) = file.read_exact(&mut buf).map_err(|e| {
                anyhow!(
                    "unable to read {} byte(s) from {} at offset {:#x}: {}",
                    memory_map.size,
                    path,
                    memory_map.base_address,
                    e
                )
            }) {
                eprintln!("error: {}", e);
                continue;
            }
            memory_map.data = buf;
        }

        Ok(memory_maps)
    }
}

impl Drop for Tracer {
    fn drop(&mut self) {
        let _ = nix_ptrace::detach(self.pid, None);
    }
}
