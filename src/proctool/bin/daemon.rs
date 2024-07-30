use std::{
    io::{BufRead, BufReader},
    net::{TcpListener, TcpStream},
    os::fd::RawFd,
};

use anyhow::Result;
use nix::{fcntl, sys, unistd};
use telefork::proctool::common::{Args, PORT};

pub fn main() -> Result<()> {
    self_daemonize()?;

    let listener = TcpListener::bind(format!("127.0.0.1:{}", PORT))?;
    println!("listening");
    for stream in listener.incoming() {
        println!("handling new client");
        let result = handle_client(stream?);
        if let Err(e) = result {
            // TODO: will this output go anywhere?
            eprintln!("error: {}", e);
        } else {
            println!("closed connection");
        }
    }
    Ok(())
}

fn handle_client(stream: TcpStream) -> Result<()> {
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }

        println!("got data: {:?}", line);
        let args: Args = serde_json::from_str(&line)?;
        println!("got args: {:?}", args);
    }
    Ok(())
}

fn self_daemonize() -> Result<()> {
    sys::stat::umask(sys::stat::Mode::empty());

    if let unistd::ForkResult::Parent { .. } = unsafe{unistd::fork()}? {
        std::process::exit(0);
    }

    // procedure from Advanced Programming in the Unix Environment, ch. 13 sec. 3
    unistd::setsid()?;
    // TODO: real path
    unistd::chdir("/home/ian")?;
    let (_, max_open_files) = sys::resource::getrlimit(sys::resource::Resource::RLIMIT_NOFILE)?;
    for fd in 0..max_open_files {
        let _ = unistd::close(fd as RawFd);
    }

    // stdin
    fcntl::open(
        "/dev/null",
        fcntl::OFlag::O_RDONLY | fcntl::OFlag::O_NOCTTY,
        sys::stat::Mode::empty(),
    )?;
    // stdout
    open_logfile()?;
    // stderr
    open_logfile()?;

    Ok(())
}

fn open_logfile() -> Result<()> {
    use nix::fcntl::OFlag;
    use nix::sys::stat::Mode;

    fcntl::open(
        "proctool-daemon.log",
        OFlag::O_CREAT | OFlag::O_WRONLY,
        Mode::S_IRUSR | Mode::S_IWUSR | Mode::S_IRGRP | Mode::S_IWGRP | Mode::S_IROTH,
    )?;
    Ok(())
}
