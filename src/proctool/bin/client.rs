use std::io::Write;
use std::net::TcpStream;

use anyhow::{anyhow, Result};

use clap::Parser;

use telefork::proctool::common::{Args, PORT};

fn main() -> Result<()> {
    // TODO: daemon-start, daemon-kill, daemon-logs commands

    let args = Args::parse();
    let mut daemon = Daemon::connect()?;
    let result = daemon.send_command(args);
    if let Err(e) = result {
        eprintln!("error: {}", e);
        std::process::exit(1);
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

    pub fn send_command(&mut self, args: Args) -> Result<()> {
        let data = serde_json::to_string(&args)?;
        self.stream.write(data.as_bytes())?;
        Ok(())
    }

    fn addr() -> String {
        format!("127.0.0.1:{}", PORT)
    }
}
