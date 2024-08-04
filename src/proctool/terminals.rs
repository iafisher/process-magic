use std::io::Write;

use anyhow::Result;
use nix::unistd;

pub fn get_terminal(pid: unistd::Pid) -> Result<String> {
    let stat = procfs::process::Process::new(pid.as_raw())?.stat()?;
    Ok(format!("pts/{}", stat.tty_nr().1))
}

pub fn clear_terminal(pts: &str) -> Result<()> {
    let mut f = std::fs::File::options().write(true).open(format!("/dev/{}", pts))?;
    // clear the screen: "\033[2J"
    f.write(&[0o33, 91, 50, 74])?;
    // return cursor to top left: "\033[1;1H"
    f.write(&[0o33, 91, 49, 59, 49, 72])?;
    Ok(())
}
