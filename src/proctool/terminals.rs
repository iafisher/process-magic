use std::{ffi::CString, io::Write};

use anyhow::{anyhow, Result};
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

/// returns (rows, columns)
pub fn get_terminal_size(pts: &str) -> Result<(u16, u16)> {
    let fd = fcntl::open(
        CString::new(pts)?.as_c_str(),
        fcntl::OFlag::O_WRONLY,
        sys::stat::Mode::empty(),
    )?;

    let winsize = libc::winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    unsafe {
        libc::ioctl(fd, libc::TIOCGWINSZ, &winsize);
    }

    Ok((winsize.ws_row, winsize.ws_col))
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

pub fn spawn_on_terminal(args: Vec<String>, tty: String) -> Result<()> {
    let tty = normalize_tty(&tty)?;
    let path = CString::new(args[0].clone())?;
    let tty_c = CString::new(tty.clone())?;
    let args_c: Vec<CString> = args
        .iter()
        .map(|a| CString::new(a.clone()).unwrap())
        .collect();

    // TODO: this should actually properly attach to the terminal's foreground process group

    // https://blog.nelhage.com/2011/02/changing-ctty/
    match unsafe { unistd::fork() }? {
        unistd::ForkResult::Parent { child } => {
            sys::wait::waitpid(child, Some(sys::wait::WaitPidFlag::WSTOPPED))?;
            println!("current pgid: {}", unistd::getpgid(None).unwrap());
            let child_pgid = unistd::getpgid(Some(child))?;
            println!("child pgid: {}", child_pgid);
            unistd::setpgid(unistd::Pid::from_raw(0), child_pgid)?;
            println!("new pgid: {}", unistd::getpgid(None).unwrap());
            unistd::setsid()?;
            println!("my pid: {}", unistd::getpid());

            // let mut x = String::new();
            // std::io::stdin().read_line(&mut x).unwrap();

            // unistd::close(1)?;
            let fd = fcntl::open(
                tty_c.as_c_str(),
                fcntl::OFlag::O_RDWR,
                sys::stat::Mode::empty(),
            )?;
            let r = unsafe { libc::ioctl(fd, libc::TIOCSCTTY, 1) };
            if r == -1 {
                let msg = CString::new("ioctl")?;
                unsafe {
                    libc::perror(msg.as_ptr());
                }
            }

            println!("ioctl returned {}", r);
            sys::signal::kill(child, sys::signal::Signal::SIGKILL)?;
            let _ = clear_terminal(&tty);
            let _ = unistd::execv(&path, &args_c);
        }
        unistd::ForkResult::Child => {
            let _ = unistd::setpgid(unistd::Pid::from_raw(0), unistd::Pid::from_raw(0));
            let _ = sys::signal::raise(sys::signal::SIGSTOP);
        }
    }

    Ok(())

    // let _ = unistd::close(1);
    // let _ = unistd::close(2);

    // let _ = fcntl::open(
    //     tty_c.as_c_str(),
    //     fcntl::OFlag::O_WRONLY,
    //     sys::stat::Mode::empty(),
    // );
    // let _ = fcntl::open(
    //     tty_c.as_c_str(),
    //     fcntl::OFlag::O_WRONLY,
    //     sys::stat::Mode::empty(),
    // );

    // let _ = clear_terminal(&tty);
    // let _ = unistd::execv(&path, &args_c);
    // std::process::exit(1);
}

pub fn normalize_tty(tty: &str) -> Result<String> {
    if tty.starts_with("/dev/pts/") {
        Ok(tty.to_string())
    } else if tty.starts_with("pts/") {
        Ok(format!("/dev/{}", tty))
    } else {
        Err(anyhow!("could not interpret {:?} as a TTY identifier", tty))
    }
}
