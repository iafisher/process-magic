use anyhow::Result;
use clap::Parser;

use process_magic::common::httpapi;
use process_magic::teleclient::ptrace;

#[derive(Parser, Debug)]
struct Args {
    #[arg(short, long)]
    pid: i32,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let tracer = ptrace::Tracer::seize_and_interrupt(args.pid)?;
    let gp_register_data = tracer.get_general_purpose_registers()?;
    let fp_register_data = tracer.get_floating_point_registers()?;
    let memory_maps = tracer.read_memory()?;

    let client = reqwest::blocking::Client::new();
    let request = httpapi::TeleforkApiRequest {
        gp_register_data,
        fp_register_data,
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
