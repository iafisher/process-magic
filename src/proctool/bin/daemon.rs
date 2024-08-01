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
use syscalls::Sysno;
use telefork::{
    proctool::common::{Args, DaemonMessage, PORT},
    teleclient::procfs,
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
        Args::Pause(args) => {
            let pid = unistd::Pid::from_raw(args.pid);
            sys::ptrace::attach(pid)?;
        }
        Args::Resume(args) => {
            let pid = unistd::Pid::from_raw(args.pid);
            sys::ptrace::detach(pid, None)?;
        }
        Args::Takeover(args) => {
            let pid = unistd::Pid::from_raw(args.pid);
            sys::ptrace::attach(pid)?;
            sys::wait::waitpid(pid, Some(sys::wait::WaitPidFlag::WSTOPPED))
                .map_err(|e| anyhow!("failed to waitpid (set-up): {}", e))?;

            let mut registers = sys::ptrace::getregset::<sys::ptrace::regset::NT_PRSTATUS>(pid)
                .map_err(|e| anyhow!("PTRACE_GETREGSET failed: {}", e))?;
            let str_addr = inject_string_constant(pid, format!("{}/bin/takeover", root))
                .map_err(|e| anyhow!("failed to inject string constant: {}", e))?;

            // syscall number in x8, args in x0, x1, x2, x3...
            registers.regs[8] = Sysno::execve.id() as u64;
            registers.regs[0] = str_addr;
            registers.regs[1] = 0;
            registers.regs[2] = 0;

            let p = registers.pc as *mut libc::c_void;
            sys::ptrace::setregset::<sys::ptrace::regset::NT_PRSTATUS>(pid, registers)
                .map_err(|e| anyhow!("PTRACE_SETREGSET failed: {}", e))?;

            let new_instructions: u64 = 0xd4000001d4000001;
            log::info!("overwriting at {:#x}", registers.pc);
            // 0xd4000001 = svc #0
            sys::ptrace::write(pid, p, new_instructions as i64)
                .map_err(|e| anyhow!("PTRACE_POKEDATA failed (syscall injection): {}", e))?;

            // TODO: remove this debugging switch
            let pause_to_inspect = false;
            if !pause_to_inspect {
                sys::ptrace::step(pid, None)
                    .map_err(|e| anyhow!("PTRACE_SINGLESTEP failed: {}", e))?;
                sys::wait::waitpid(pid, Some(sys::wait::WaitPidFlag::WSTOPPED))
                    .map_err(|e| anyhow!("failed to waitpid (syscall injection): {}", e))?;
                sys::ptrace::detach(pid, None)
                    .map_err(|e| anyhow!("PTRACE_DETACH failed: {}", e))?;
            } else {
                sys::ptrace::detach(pid, Some(sys::signal::Signal::SIGSTOP))
                    .map_err(|e| anyhow!("PTRACE_DETACH failed: {}", e))?;
            }
        }
        _ => {
            return Err(anyhow!("unknown command {:?}", args));
        }
    }

    Ok(())
}

fn inject_string_constant(pid: unistd::Pid, s: String) -> Result<u64> {
    let addr = find_addr_for_string_constant(pid, s.len())?;

    // would be more efficient to use `process_vm_writev` but string should be short and this is simpler
    let p = addr as *mut libc::c_void;
    let mut offset = 0;
    for b in s.as_bytes() {
        sys::ptrace::write(pid, p.wrapping_byte_offset(offset), *b as i64)
            .map_err(|e| anyhow!("PTRACE_POKEDATA failed: {}", e))?;
        offset += 1;
    }
    Ok(addr)
}

fn find_addr_for_string_constant(pid: unistd::Pid, length: usize) -> Result<u64> {
    let memory_maps = procfs::read_memory_maps(pid.as_raw())?;
    for memory_map in memory_maps {
        if memory_map.size < length as u64 {
            continue;
        }

        if !memory_map.readable || !memory_map.writable {
            continue;
        }

        // don't want to overwrite important segments which are usually labelled (stack, heap, vdso, etc.)
        if !memory_map.label.is_empty() {
            continue;
        }

        return Ok(memory_map.base_address);
    }
    Err(anyhow!("no suitable memory region found"))
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
