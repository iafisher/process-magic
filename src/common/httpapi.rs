use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct TeleforkApiRequest {
    // TODO: include instruction set (x86-64 or arm)
    // https://doc.rust-lang.org/reference/conditional-compilation.html
    pub index: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TeleforkApiResponse {
    pub status: u32,
}
