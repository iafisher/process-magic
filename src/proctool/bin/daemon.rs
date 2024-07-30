use std::{
    io::{BufRead, BufReader},
    net::{TcpListener, TcpStream},
    os::fd::RawFd,
};

use anyhow::{anyhow, Result};
use log::LevelFilter;
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Root},
    encode::pattern::PatternEncoder,
    Config,
};
use nix::{fcntl, sys, unistd};
use telefork::proctool::common::{Args, DaemonMessage, PORT};

pub fn main() -> Result<()> {
    let root = std::env::var("PROCTOOL_ROOT").or(Err(anyhow!("PROCTOOL_ROOT must be set")))?;

    self_daemonize(&root)?;
    configure_logging(&root)?;

    let listener = TcpListener::bind(format!("127.0.0.1:{}", PORT))?;
    log::info!("listening on port {}", PORT);
    for stream in listener.incoming() {
        log::info!("new TCP connection");
        match handle_client(stream?) {
            Ok(should_shutdown) => {
                if should_shutdown {
                    log::info!("shutting down due to client request");
                    break;
                }
            }
            Err(e) => {
                log::error!("error while servicing request: {}", e);
            }
        }
    }
    Ok(())
}

// returns true if server should shut down
fn handle_client(stream: TcpStream) -> Result<bool> {
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }

        let message: DaemonMessage = serde_json::from_str(&line)?;

        match message {
            DaemonMessage::Command(args) => {
                let result = run_command(args);
                if let Err(e) = result {
                    log::error!("failed to run command: {}", e);
                }
            }
            DaemonMessage::Kill => {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn run_command(args: Args) -> Result<()> {
    match args {
        Args::Pause(args) => {
            let pid = unistd::Pid::from_raw(args.pid);
            sys::ptrace::attach(pid)?;
        }
        Args::Resume(args) => {
            let pid = unistd::Pid::from_raw(args.pid);
            sys::ptrace::detach(pid, None)?;
        }
        _ => {
            return Err(anyhow!("unknown command {:?}", args));
        }
    }

    Ok(())
}

fn self_daemonize(root: &str) -> Result<()> {
    sys::stat::umask(sys::stat::Mode::empty());

    if let unistd::ForkResult::Parent { .. } = unsafe { unistd::fork() }? {
        std::process::exit(0);
    }

    // procedure from Advanced Programming in the Unix Environment, ch. 13 sec. 3
    unistd::setsid()?;
    // TODO: real path
    unistd::chdir(root)?;
    let (_, max_open_files) = sys::resource::getrlimit(sys::resource::Resource::RLIMIT_NOFILE)?;
    for fd in 0..max_open_files {
        let _ = unistd::close(fd as RawFd);
    }

    // stdin
    open_devnull()?;
    // stdout
    open_devnull()?;
    // stderr
    open_devnull()?;

    Ok(())
}

fn configure_logging(root: &str) -> Result<()> {
    let file_appender = FileAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{d} {l} - {m}")))
        // TODO: real path
        .build(format!("{}/daemon.log", root))?;

    let config = Config::builder()
        .appender(Appender::builder().build("main", Box::new(file_appender)))
        .build(Root::builder().appender("main").build(LevelFilter::Info))?;

    log4rs::init_config(config)?;

    Ok(())
}

fn open_devnull() -> Result<()> {
    fcntl::open(
        "/dev/null",
        fcntl::OFlag::O_RDONLY | fcntl::OFlag::O_NOCTTY,
        sys::stat::Mode::empty(),
    )?;
    Ok(())
}
