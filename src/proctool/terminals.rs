use anyhow::Result;
use nix::unistd;

pub fn get_terminal(pid: unistd::Pid) -> Result<String> {
    let stat = procfs::process::Process::new(pid.as_raw())?.stat()?;
    Ok(format!("pts/{}", stat.tty_nr().1))
}
