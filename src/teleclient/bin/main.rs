use anyhow::Result;
use clap::Parser;

use telefork::common::httpapi;
use telefork::teleclient::ptrace;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    pid: i32,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let tracer = ptrace::Tracer::seize_and_interrupt(args.pid)?;
    let register_data = tracer.get_registers()?;
    let memory_maps = tracer.read_memory()?;

    let client = reqwest::blocking::Client::new();
    let request = httpapi::TeleforkApiRequest {
        register_data,
        memory_maps,
    };
    let response: httpapi::TeleforkApiResponse = client
        .post("http://localhost:8000/telefork")
        .json(&request)
        .send()?
        .json()?;

    if !response.success {
        eprintln!("error: remote call was not successful");
        std::process::exit(1);
    }

    Ok(())
}
