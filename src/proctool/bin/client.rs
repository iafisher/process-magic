use std::io::Write;
use std::net::TcpStream;
use std::process::Command;

use anyhow::{anyhow, Result};

use clap::Parser;

use telefork::proctool::common::{Args, DaemonMessage, PORT};

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
