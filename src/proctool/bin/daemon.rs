use std::{
    cell::OnceCell,
    io::{BufRead, BufReader, IoSlice, IoSliceMut},
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
    teleclient::procfs::{self, MemoryMap},
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
        Args::Redirect(args) => {
            // TODO: actually select an eligible terminal
            let terminal = "/dev/pts/4";
            let pid = unistd::Pid::from_raw(args.pid);
            let controller = ProcessController::new(pid);

            controller.attach()?;
            controller.ensure_not_in_syscall()?;
            let original_registers = controller.get_registers()?;

            controller.execute_syscall(Sysno::close, vec![1])?;

            let str_addr = controller.inject_bytes(format!("{}\0", terminal).as_bytes())?;
            // ARM64 doesn't have open() syscall
            controller.execute_syscall(
                Sysno::openat,
                vec![0, str_addr as i64, libc::O_WRONLY as i64, 0],
            )?;

            controller.set_registers(original_registers)?;
            controller.detach()?;
        }
        Args::Rewind(args) => {
            todo!()
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
        _ => {
            return Err(anyhow!("unknown command {:?}", args));
        }
    }

    Ok(())
}

struct ProcessController {
    pid: unistd::Pid,
    memory_maps: OnceCell<Vec<MemoryMap>>,
}

impl ProcessController {
    pub fn new(pid: unistd::Pid) -> Self {
        Self {
            pid,
            memory_maps: OnceCell::new(),
        }
    }

    pub fn attach(&self) -> Result<()> {
        sys::ptrace::attach(self.pid).map_err(|e| anyhow!("PTRACE_ATTACH failed: {}", e))?;
        sys::wait::waitpid(self.pid, Some(sys::wait::WaitPidFlag::WSTOPPED))
            .map_err(|e| anyhow!("failed to waitpid after PTRACE_ATTACH: {}", e))?;
        Ok(())
    }

    pub fn ensure_not_in_syscall(&self) -> Result<()> {
        let initial_registers = self.get_registers()?;
        let initial_pc = initial_registers.pc;

        loop {
            self.step_and_wait()?;
            let current_registers = self.get_registers()?;
            if current_registers.pc != initial_pc {
                break;
            }
            // TODO: sleep for an interval and have a timeout
            // would also be nice to return to the user that the program may need user interaction
        }

        Ok(())
    }

    pub fn prepare_syscall(&self, sysno: Sysno, args: Vec<i64>) -> Result<()> {
        let new_pc = self.find_svc_instruction()?;

        let mut registers = self.get_registers()?;

        // syscall number in x8, args in x0, x1, x2, x3...
        registers.regs[8] = sysno.id() as u64;
        for i in 0..args.len() {
            registers.regs[i] = args[i] as u64;
        }
        registers.pc = new_pc;

        self.set_registers(registers)?;
        Ok(())
    }

    fn execute_syscall(&self, sysno: Sysno, args: Vec<i64>) -> Result<u64> {
        self.prepare_syscall(sysno, args)?;
        self.ensure_not_in_syscall()?;
        let registers = self.get_registers()?;
        Ok(registers.regs[0])
    }

    pub fn find_svc_instruction(&self) -> Result<u64> {
        for memory_map in self.get_memory_maps()? {
            // [vdso] section should always have a syscall instruction
            // more robust: look at every executable section
            if memory_map.label == "[vdso]" {
                return find_svc_instruction_in_map(self.pid, &memory_map);
            }
        }

        Err(anyhow!("could not find [vdso] segment in binary"))
    }

    pub fn get_registers(&self) -> Result<libc::user_regs_struct> {
        let registers = sys::ptrace::getregset::<sys::ptrace::regset::NT_PRSTATUS>(self.pid)
            .map_err(|e| anyhow!("PTRACE_GETREGSET failed: {}", e))?;
        Ok(registers)
    }

    pub fn set_registers(&self, registers: libc::user_regs_struct) -> Result<()> {
        sys::ptrace::setregset::<sys::ptrace::regset::NT_PRSTATUS>(self.pid, registers)
            .map_err(|e| anyhow!("PTRACE_SETREGSET failed: {}", e))?;
        Ok(())
    }

    pub fn step_and_wait(&self) -> Result<()> {
        sys::ptrace::step(self.pid, None)
            .map_err(|e| anyhow!("PTRACE_SINGLESTEP failed: {}", e))?;
        sys::wait::waitpid(self.pid, Some(sys::wait::WaitPidFlag::WSTOPPED))
            .map_err(|e| anyhow!("failed to waitpid (syscall injection): {}", e))?;
        Ok(())
    }

    pub fn inject_bytes(&self, bytes: &[u8]) -> Result<u64> {
        let addr = self.execute_syscall(
            Sysno::mmap,
            vec![
                0,
                bytes.len() as i64,
                (libc::PROT_READ | libc::PROT_WRITE) as i64,
                (libc::MAP_ANON | libc::MAP_PRIVATE) as i64,
                -1,
                0,
            ],
        )?;

        let local_iov = IoSlice::new(bytes);
        let remote_iov = sys::uio::RemoteIoVec {
            base: addr as usize,
            len: bytes.len(),
        };
        let nwritten = sys::uio::process_vm_writev(self.pid, &[local_iov], &[remote_iov])?;
        if nwritten == 0 {
            return Err(anyhow!("failed to write data"));
        }

        Ok(addr)
    }

    pub fn detach(&self) -> Result<()> {
        self.detach_generic(None)
    }

    pub fn detach_and_stop(&self) -> Result<()> {
        self.detach_generic(Some(sys::signal::Signal::SIGSTOP))
    }

    fn detach_generic(&self, signal: Option<sys::signal::Signal>) -> Result<()> {
        sys::ptrace::detach(self.pid, signal)
            .map_err(|e| anyhow!("PTRACE_DETACH failed: {}", e))?;
        Ok(())
    }

    fn get_memory_maps(&self) -> Result<&Vec<MemoryMap>> {
        self.initialize_memory_maps()?;
        Ok(self.memory_maps.get().unwrap())
    }

    fn initialize_memory_maps(&self) -> Result<()> {
        if self.memory_maps.get().is_some() {
            return Ok(());
        }

        let memory_maps = procfs::read_memory_maps(self.pid.as_raw())?;
        let _ = self.memory_maps.set(memory_maps);
        Ok(())
    }
}

fn find_svc_instruction_in_map(pid: unistd::Pid, memory_map: &MemoryMap) -> Result<u64> {
    let mut buffer = vec![0; memory_map.size as usize];
    let local_iov = &mut [IoSliceMut::new(&mut buffer[..])];
    let remote_iov = sys::uio::RemoteIoVec {
        base: memory_map.base_address as usize,
        len: memory_map.size as usize,
    };
    let nread = sys::uio::process_vm_readv(pid, local_iov, &[remote_iov])?;
    if nread == 0 {
        return Err(anyhow!("failed to read any data"));
    }

    // value: 0xd4000001
    // little-endian representation: 0x01 0x00 0x00 0xd4

    for i in 0..buffer.len() - 3 {
        if buffer[i] == 0x01
            && buffer[i + 1] == 0x00
            && buffer[i + 2] == 0x00
            && buffer[i + 3] == 0xd4
        {
            return Ok(memory_map.base_address + i as u64);
        }
    }

    Err(anyhow!("could not find svc instruction in segment"))
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
