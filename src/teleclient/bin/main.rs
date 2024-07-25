use anyhow::Result;
use clap::Parser;

use telefork::common::httpapi;
use telefork::teleclient::{procfs, ptrace};

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    pid: i32,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let memory_maps = procfs::read_memory_maps(args.pid)?;
    println!("{:?}", memory_maps);

    let tracer = ptrace::Tracer::seize_and_interrupt(args.pid)?;
    tracer.get_registers()?;

    let client = reqwest::blocking::Client::new();
    let request = httpapi::TeleforkApiRequest { index: 42 };
    let response: httpapi::TeleforkApiResponse = client
        .post("http://localhost:8000/telefork")
        .json(&request)
        .send()?
        .json()?;

    println!("Got response: {:?}", response);
    Ok(())
}
