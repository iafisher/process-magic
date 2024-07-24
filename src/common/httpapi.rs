use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct TeleforkApiRequest {
    pub index: u32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TeleforkApiResponse {
    pub status: u32,
}
