use anyhow::Result;

use telefork::common::httpapi;

fn main() -> Result<()> {
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
