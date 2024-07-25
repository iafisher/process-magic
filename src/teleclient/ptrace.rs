use std::mem::MaybeUninit;

use anyhow::{anyhow, Result};
use nix::sys::ptrace as nix_ptrace;
use nix::sys::wait::WaitPidFlag;
use nix::unistd::Pid;
use syscalls::Sysno;

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

    pub fn get_registers(&self) -> Result<Vec<u8>> {
        // adapted from https://github.com/facebookexperimental/reverie/blob/852e08e75ddcd0ca3f5ea0ded7e60491051ffb76/safeptrace/src/lib.rs#L515

        // TODO: what does MaybeUninit mean?
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
                libc::NT_PRSTATUS,
                // TODO: what does this mean?
                &mut iov as *mut _
            )
        }
        .map_err(|e| anyhow!("failed to get register set: {}", e))?;

        let mut r = Vec::new();
        for i in 0..iov.iov_len as isize {
            let p = iov.iov_base as *mut u8;
            let v = unsafe { *p.offset(i) };
            r.push(v);
        }

        Ok(r)
    }
}

impl Drop for Tracer {
    fn drop(&mut self) {
        let _ = nix_ptrace::detach(self.pid, None);
    }
}
