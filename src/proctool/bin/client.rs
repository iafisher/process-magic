use std::io::BufReader;
use std::net::TcpStream;
use std::process::Command;
use std::{fs, io::Write};

use anyhow::{anyhow, Result};

use clap::Parser;

use nix::{sys, unistd};
use process_magic::proctool::pcontroller::ProcessController;
use process_magic::proctool::{
    common::{Args, DaemonMessage, PORT},
    cryogenics, procinfo, terminals,
};

fn main() -> Result<()> {
    let args = Args::parse();

    let root = std::env::var("PROCTOOL_ROOT").or(Err(anyhow!("PROCTOOL_ROOT must be set")))?;

    // TODO: daemon-restart
    match args {
        Args::DaemonKill => {
            kill_daemon()?;
        }
        Args::DaemonLogs => {
            follow_daemon_logs(&root)?;
        }
        Args::DaemonRestart => {
            kill_daemon()?;
            start_daemon(&root)?;
        }
        Args::DaemonStart => {
            start_daemon(&root)?;
        }
        Args::DaemonStatus => {
            print_daemon_status();
        }
        Args::Groups => {
            procinfo::print_process_groups()?;
        }
        Args::Processes(args) => {
            if let Some(pid) = args.pid {
                procinfo::print_process_tree(pid)?;
            } else {
                procinfo::list_processes()?;
            }
        }
        Args::Sessions => {
            procinfo::print_sessions()?;
        }
        Args::Terminals => {
            procinfo::list_terminals()?;
        }
        Args::Which => {
            print_what_terminal()?;
        }
        Args::Freeze(args) => {
            let state = cryogenics::freeze(unistd::Pid::from_raw(args.pid))?;

            // TODO: don't save file as root
            // This doesn't work:
            //   unistd::setuid(unistd::getuid())?;
            let fname = format!("{}.state", args.pid);
            let mut f = fs::File::options().write(true).create(true).open(&fname)?;

            serde_json::to_writer(&mut f, &state)?;
            println!("Saved to {}", fname);
        }
        Args::Thaw(args) => {
            let f = fs::File::open(&args.path)?;
            let mut reader = BufReader::new(f);
            let state: cryogenics::ProcessState = serde_json::from_reader(&mut reader)?;
            cryogenics::thaw(&state)?;
        }
        Args::UnmapChild => match unsafe { unistd::fork() }? {
            unistd::ForkResult::Parent { child } => {
                sys::wait::waitpid(child, Some(sys::wait::WaitPidFlag::WSTOPPED))
                    .map_err(|e| anyhow!("failed to waitpid: {}", e))?;
                println!("child pid: {}", child);

                let controller = ProcessController::new(child);
                let svc_region_addr = controller
                    .map_svc_region()
                    .map_err(|e| anyhow!("failed to map svc region: {}", e))?;

                controller.unmap_existing_regions(svc_region_addr)?;
                controller.detach_and_stop()?;
                controller.waitpid()?;
            }
            unistd::ForkResult::Child => {
                sys::ptrace::traceme()?;
                sys::signal::raise(sys::signal::SIGSTOP)?;
            }
        },
        Args::Oblivion(_) => {
            dispatch_to_daemon(args)?;
            Command::new(format!("{}/bin/oblivion", root)).arg("3").spawn()?;
        }
        _ => {
            dispatch_to_daemon(args)?;
        }
    }

    Ok(())
}

fn kill_daemon() -> Result<()> {
    let mut daemon = Daemon::connect()?;
    let result = daemon.send_message(DaemonMessage::Kill);
    if let Err(e) = result {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}

fn start_daemon(root: &str) -> Result<()> {
    let mut cmd = Command::new("sudo")
        .arg(format!("PROCTOOL_ROOT={}", root))
        .arg(format!("{}/bin/proctool-daemon", root))
        .spawn()?;
    cmd.wait()?;
    Ok(())
}

fn follow_daemon_logs(root: &str) -> Result<()> {
    let mut cmd = Command::new("tail")
        .arg("-F")
        .arg(format!("{}/daemon.log", root))
        .spawn()?;
    cmd.wait()?;
    Ok(())
}

fn dispatch_to_daemon(args: Args) -> Result<()> {
    let mut daemon = Daemon::connect()?;
    let result = daemon.send_message(DaemonMessage::Command(args));
    if let Err(e) = result {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
    Ok(())
}

fn print_daemon_status() {
    if Daemon::connect().is_ok() {
        println!("daemon is running");
    } else {
        println!("daemon is not running");
        std::process::exit(1);
    }
}

fn print_what_terminal() -> Result<()> {
    let terminal = terminals::get_terminal(unistd::Pid::this())?;
    println!("{}", terminal);
    Ok(())
}

struct Daemon {
    stream: TcpStream,
}

impl Daemon {
    pub fn connect() -> Result<Self> {
        let stream = TcpStream::connect(Self::addr())
            .map_err(|e| anyhow!("could not connect to daemon: {}", e))?;
        Ok(Self { stream })
    }

    pub fn send_message(&mut self, msg: DaemonMessage) -> Result<()> {
        let data = serde_json::to_string(&msg)?;
        self.stream.write(data.as_bytes())?;
        Ok(())
    }

    fn addr() -> String {
        format!("127.0.0.1:{}", PORT)
    }
}
