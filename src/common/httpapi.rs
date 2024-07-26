use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct TeleforkApiRequest {
    // TODO: include instruction set (x86-64 or arm)
    // https://doc.rust-lang.org/reference/conditional-compilation.html
    pub index: u32,
    // unstructured and processor-dependent; only intended to be passed back to ptrace()
    pub register_data: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TeleforkApiResponse {
    pub status: u32,
}
