use anyhow::{anyhow, Result};
use nix::sys::ptrace as nix_ptrace;
use nix::sys::signal::{self, Signal};
use nix::sys::wait::WaitPidFlag;
use nix::unistd::fork;

pub fn spawn_process() -> Result<()> {
    println!("spawn_process");
    match unsafe { fork() }? {
        nix::unistd::ForkResult::Parent { child } => {
            println!("in parent, child PID is {}", child);
            nix::sys::wait::waitpid(child, Some(WaitPidFlag::WSTOPPED))
                .map_err(|e| anyhow!("failed to waitpid: {}", e))?;

            // TODO: registers: ptrace()
            // TODO: memory: https://docs.rs/nix/0.29.0/nix/sys/uio/fn.process_vm_writev.html
            println!("tracing child");

            signal::kill(child, Signal::SIGKILL)
                .map_err(|e| anyhow!("unable to kill child process: {}", e))?;
        }
        nix::unistd::ForkResult::Child => {
            nix_ptrace::traceme().map_err(|e| anyhow!("failed to ptrace child: {}", e))?;
            signal::raise(Signal::SIGSTOP)?;
        }
    }

    Ok(())
}
