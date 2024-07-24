use rocket::serde::{json::Json, Deserialize, Serialize};

#[macro_use]
extern crate rocket;

#[derive(Deserialize, Debug)]
#[serde(crate = "rocket::serde")]
struct TeleforkApiRequest {
    index: u32,
}

#[derive(Serialize, Debug)]
#[serde(crate = "rocket::serde")]
struct TeleforkApiResponse {
    status: u32,
}

#[post("/telefork", data = "<request>")]
fn telefork_route(request: Json<TeleforkApiRequest>) -> Json<TeleforkApiResponse> {
    println!("{:?}", request);
    Json(TeleforkApiResponse { status: 1 })
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount("/", routes![telefork_route])
}
