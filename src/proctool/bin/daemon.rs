use std::{
    cell::OnceCell,
    io::{BufRead, BufReader, IoSliceMut},
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

            controller.set_up_syscall(Sysno::close, vec![1])?;
            controller.step_and_wait()?;

            let (str_addr, _) = controller.inject_string_constant(terminal.to_string())?;
            // ARM64 doesn't have open() syscall
            controller.set_up_syscall(Sysno::openat, vec![0, str_addr, libc::O_WRONLY as u64, 0])?;

            controller.step_and_wait()?;
            controller.set_registers(original_registers)?;
            controller.detach()?;
        }
        Args::Takeover(args) => {
            let pid = unistd::Pid::from_raw(args.pid);
            let controller = ProcessController::new(pid);

            controller.attach()?;
            controller.ensure_not_in_syscall()?;

            let path_to_program = args.bin.unwrap_or(format!("{}/bin/takeover", root));
            let (str_addr, empty_addr) =
                controller.inject_string_constant(path_to_program)?;
            let syscall_args = vec![str_addr, empty_addr, empty_addr];
            controller.set_up_syscall(Sysno::execve, syscall_args)?;

            if !args.pause {
                controller.step_and_wait()?;
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
            sys::ptrace::step(self.pid, None)
                .map_err(|e| anyhow!("PTRACE_SINGLESTEP failed: {}", e))?;
            sys::wait::waitpid(self.pid, Some(sys::wait::WaitPidFlag::WSTOPPED))
                .map_err(|e| anyhow!("failed to waitpid (ensure_not_in_syscall): {}", e))?;
            let current_registers = self.get_registers()?;
            if current_registers.pc != initial_pc {
                break;
            }
            // TODO: sleep for an interval and have a timeout
            // would also be nice to return to the user that the program may need user interaction
        }

        Ok(())
    }

    pub fn set_up_syscall(&self, sysno: Sysno, args: Vec<u64>) -> Result<()> {
        let new_pc = self.find_svc_instruction()?;

        let mut registers = self.get_registers()?;

        // syscall number in x8, args in x0, x1, x2, x3...
        registers.regs[8] = sysno.id() as u64;
        for i in 0..args.len() {
            registers.regs[i] = args[i];
        }
        registers.pc = new_pc;

        self.set_registers(registers)?;
        Ok(())
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

    pub fn inject_string_constant(&self, s: String) -> Result<(u64, u64)> {
        let addr = find_addr_for_string_constant(self.pid, s.len())?;

        // would be more efficient to use `process_vm_writev` but string should be short and this is simpler
        let p = addr as *mut libc::c_void;
        let mut offset = 0;
        for b in s.as_bytes() {
            sys::ptrace::write(self.pid, p.wrapping_byte_offset(offset), *b as i64)
                .map_err(|e| anyhow!("PTRACE_POKEDATA failed: {}", e))?;
            offset += 1;
        }

        let null_terminator_addr = p.wrapping_byte_offset(offset);
        sys::ptrace::write(self.pid, null_terminator_addr, 0)
            .map_err(|e| anyhow!("PTRACE_POKEDATA failed: {}", e))?;

        // for execve we also need a pointer to an array whose only element is NULL; conveniently, a
        // pointer to the null terminator at the end of the string works for this.
        //
        // the alternative -- injecting a separate empty array -- requires us to remember where we
        // wrote the string constant so we don't overwrite it.
        Ok((addr, null_terminator_addr as u64))
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
