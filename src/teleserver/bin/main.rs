use rocket::serde::json::Json;

use telefork::common::httpapi;

#[macro_use]
extern crate rocket;

#[post("/telefork", data = "<request>")]
fn telefork_route(request: Json<httpapi::TeleforkApiRequest>) -> Json<httpapi::TeleforkApiResponse> {
    println!("{:?}", request);
    Json(httpapi::TeleforkApiResponse { status: 1 })
}

#[launch]
fn rocket() -> _ {
    rocket::build().mount("/", routes![telefork_route])
}
