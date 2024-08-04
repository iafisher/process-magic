use std::{ffi::CString, io::Write};

use anyhow::Result;
use nix::{fcntl, sys, unistd};

pub fn get_terminal(pid: unistd::Pid) -> Result<String> {
    let stat = procfs::process::Process::new(pid.as_raw())?.stat()?;
    Ok(format!("/dev/pts/{}", stat.tty_nr().1))
}

pub fn clear_terminal(pts: &str) -> Result<()> {
    let mut f = std::fs::File::options().write(true).open(pts)?;
    // clear the screen: "\033[2J"
    f.write(&[0o33, 91, 50, 74])?;
    // return cursor to top left: "\033[1;1H"
    f.write(&[0o33, 91, 49, 59, 49, 72])?;
    Ok(())
}

pub fn write_to_stdin(pid: unistd::Pid, line: &str) -> Result<()> {
    let fpath = CString::new(format!("/proc/{}/fd/0", pid))?;
    let fd = fcntl::open(
        fpath.as_c_str(),
        fcntl::OFlag::O_WRONLY,
        sys::stat::Mode::empty(),
    )?;

    for byte in line.as_bytes() {
        unsafe {
            libc::ioctl(fd, libc::TIOCSTI, byte);
        }
    }

    let newline = 0x0a;
    unsafe {
        libc::ioctl(fd, libc::TIOCSTI, &newline);
    }

    unistd::close(fd)?;
    Ok(())
}
