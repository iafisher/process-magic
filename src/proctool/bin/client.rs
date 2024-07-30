use std::io::Write;
use std::net::TcpStream;
use std::process::Command;

use anyhow::{anyhow, Result};

use clap::Parser;

use telefork::proctool::common::{Args, DaemonMessage, PORT};

fn main() -> Result<()> {
    let args = Args::parse();

    match args {
        Args::DaemonKill(_) => {
            let mut daemon = Daemon::connect()?;
            let result = daemon.send_message(DaemonMessage::Kill);
            if let Err(e) = result {
                eprintln!("error: {}", e);
                std::process::exit(1);
            }
        }
        Args::DaemonLogs(_) => {
            let mut cmd = Command::new("tail")
                .arg("-f")
                .arg("/home/ian/proctool-daemon.log")
                .spawn()?;
            cmd.wait()?;
        }
        Args::DaemonStart(_) => {
            let mut cmd = Command::new("sudo")
                // TODO: real path
                .arg("target/debug/proctool-daemon")
                .spawn()?;
            cmd.wait()?;
        }
        Args::DaemonStatus(_) => {
            if Daemon::connect().is_ok() {
                println!("daemon is running");
            } else {
                println!("daemon is not running");
                std::process::exit(1);
            }
        }
        _ => {
            // command handled by daemon
            let mut daemon = Daemon::connect()?;
            let result = daemon.send_message(DaemonMessage::Command(args));
            if let Err(e) = result {
                eprintln!("error: {}", e);
                std::process::exit(1);
            }
        }
    }

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
