use serde::{Deserialize, Serialize};

use crate::teleclient::procfs::MemoryMap;

#[derive(Serialize, Deserialize, Debug)]
pub struct TeleforkApiRequest {
    // TODO: include instruction set (x86-64 or arm)
    // https://doc.rust-lang.org/reference/conditional-compilation.html
    // unstructured and processor-dependent; only intended to be passed back to ptrace()
    pub register_data: Vec<u8>,
    pub memory_maps: Vec<MemoryMap>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TeleforkApiResponse {
    pub success: bool,
}
