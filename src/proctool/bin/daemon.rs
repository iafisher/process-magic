use std::{
    fs,
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
use syscalls::Sysno;
use process_magic::{
    proctool::{
        common::{Args, DaemonMessage, PORT},
        pcontroller::ProcessController,
        procinfo,
        terminals::{self, write_to_stdin},
    },
    teleclient::myprocfs,
};

pub fn main() -> Result<()> {
    let root =
        std::env::var("PROCTOOL_ROOT").or(Err(anyhow!("PROCTOOL_ROOT must be set (daemon)")))?;

    self_daemonize(&root)?;
    configure_logging(&root)?;

    let result = listen_forever(&root);
    if let Err(e) = result {
        log::error!("listen_forever() exited with an error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}

pub fn listen_forever(root: &str) -> Result<()> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", PORT))?;
    log::info!("listening on port {}", PORT);
    for stream in listener.incoming() {
        log::info!("new TCP connection");
        match handle_client(root, stream?) {
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
fn handle_client(root: &str, stream: TcpStream) -> Result<bool> {
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
                let result = run_command(root, args);
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

fn run_command(root: &str, args: Args) -> Result<()> {
    match args {
        Args::Oblivion => {
            let mut biggest_terminal = String::new();
            let mut biggest_terminal_size = 0;
            for dir_entry in fs::read_dir("/dev/pts")? {
                let dir_entry = dir_entry?;
                if let Ok(s) = dir_entry.file_name().into_string() {
                    if s.parse::<u64>().is_ok() {
                        let terminal = s;
                        let (rows, cols) =
                            terminals::get_terminal_size(&format!("/dev/pts/{}", terminal))?;

                        let size = rows * cols;
                        if size >= biggest_terminal_size {
                            biggest_terminal = terminal;
                            biggest_terminal_size = size;
                        }
                    }
                }
            }

            log::info!(
                "biggest terminal: {} (size={})",
                biggest_terminal,
                biggest_terminal_size
            );
        }
        Args::Pause(args) => {
            let pid = unistd::Pid::from_raw(args.pid);
            let controller = ProcessController::new(pid);
            controller.attach()?;
        }
        Args::Resume(args) => {
            let pid = unistd::Pid::from_raw(args.pid);
            let controller = ProcessController::new(pid);
            controller.detach()?;
        }
        Args::Redirect(args) => {
            let pid = unistd::Pid::from_raw(args.pid);
            let controller = ProcessController::new(pid);

            controller.attach()?;
            controller.cancel_pending_read()?;
            let original_registers = controller.get_registers()?;

            // controller.execute_syscall(Sysno::close, vec![0])?;
            controller.execute_syscall(Sysno::close, vec![1])?;
            controller.execute_syscall(Sysno::close, vec![2])?;

            let tty = terminals::normalize_tty(&args.tty)?;
            // clearing terminal is best-effort
            let _ = terminals::clear_terminal(&tty);

            let str_addr = controller.inject_bytes(format!("{}\0", tty).as_bytes())?;
            // ARM64 doesn't have open() syscall
            // controller.execute_syscall(
            //     Sysno::openat,
            //     vec![0, str_addr as i64, libc::O_RDONLY as i64, 0],
            // )?;
            controller.execute_syscall(
                Sysno::openat,
                vec![0, str_addr as i64, libc::O_WRONLY as i64, 0],
            )?;
            controller.execute_syscall(
                Sysno::openat,
                vec![0, str_addr as i64, libc::O_WRONLY as i64, 0],
            )?;

            controller.set_registers(original_registers)?;
            controller.detach()?;
        }
        Args::Rewind(args) => {
            let pid = unistd::Pid::from_raw(args.pid);
            let controller = ProcessController::new(pid);

            controller.attach()?;
            controller.ensure_not_in_syscall()?;

            let pts = terminals::get_terminal(pid)?;
            terminals::clear_terminal(&pts)?;

            let command_line = myprocfs::get_command_line(pid.as_raw())?;

            let mut addrs = Vec::new();
            for arg in command_line {
                let addr = controller.inject_bytes(&arg)?;
                addrs.push(addr);
            }
            let argv_addr = controller.inject_u64s(&addrs)?;
            let envp_addr = controller.inject_bytes(&[0])?;

            controller.execute_syscall(
                Sysno::execve,
                vec![addrs[0] as i64, argv_addr as i64, envp_addr as i64],
            )?;
            controller.detach()?;
        }
        Args::Spawn(args) => {
            let tty = terminals::normalize_tty(&args.tty)?;
            let session_id = procinfo::get_session_id_for_terminal(&tty)?;
            terminals::write_to_stdin(unistd::Pid::from_raw(session_id), &args.cmd)?;

            // let output = Command::new("which").arg(&args.cmd[0]).output()?;
            // let mut fullpath = String::from_utf8(output.stdout)?;
            // fullpath = fullpath.trim_end_matches("\n").to_string();

            // match unsafe { unistd::fork() }? {
            //     unistd::ForkResult::Parent { .. } => {}
            //     unistd::ForkResult::Child => {
            //         if let Err(e) =
            //             terminals::spawn_on_terminal(fullpath, args.cmd, args.tty, args.uid)
            //         {
            //             log::error!("failed to spawn: {}", e);
            //         }
            //         std::process::exit(0);
            //     }
            // }
        }
        Args::Takeover(args) => {
            let pid = unistd::Pid::from_raw(args.pid);
            let controller = ProcessController::new(pid);

            controller.attach()?;
            controller.ensure_not_in_syscall()?;

            let path_to_program = args.bin.unwrap_or(format!("{}/bin/takeover", root));
            let str_addr = controller.inject_bytes(format!("{}\0", path_to_program).as_bytes())?;
            let empty_addr = controller.inject_bytes(&[0])?;
            let syscall_args = vec![str_addr as i64, empty_addr as i64, empty_addr as i64];
            controller.prepare_syscall(Sysno::execve, syscall_args)?;

            if !args.pause {
                controller.ensure_not_in_syscall()?;
                controller.detach()?;
            } else {
                controller.detach_and_stop()?;
            }
        }
        Args::WriteStdin(args) => {
            write_to_stdin(unistd::Pid::from_raw(args.pid), &args.message)?;
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
        .encoder(Box::new(PatternEncoder::new("{d} {l} - {m}\n")))
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
