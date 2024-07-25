use anyhow::Result;
use clap::Parser;

use telefork::common::httpapi;
use telefork::teleclient::procfs;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    pid: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let memory_maps = procfs::read_memory_maps(args.pid)?;
    println!("{:?}", memory_maps);

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
