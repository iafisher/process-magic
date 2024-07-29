use rocket::data::ByteUnit;
use rocket::serde::json::Json;
use rocket::Config;
use rocket::{data::Limits, http::Status};

use telefork::{common::httpapi, teleserver};

#[macro_use]
extern crate rocket;

#[post("/telefork", data = "<request>")]
fn telefork_route(
    request: Json<httpapi::TeleforkApiRequest>,
) -> (Status, Json<httpapi::TeleforkApiResponse>) {
    println!("handling request");
    if let Err(e) = teleserver::spawn::spawn_process(
        &request.gp_register_data,
        &request.fp_register_data,
        &request.memory_maps,
    ) {
        eprintln!("error: {}", e);
        return (
            Status::InternalServerError,
            Json(httpapi::TeleforkApiResponse { success: false }),
        );
    }
    println!("handling request done");

    (
        Status::Ok,
        Json(httpapi::TeleforkApiResponse { success: true }),
    )
}

#[launch]
fn rocket() -> _ {
    let limits = Limits::default().limit("json", ByteUnit::Gigabyte(8));
    let config = Config {
        limits,
        ..Config::debug_default()
    };

    rocket::build()
        .configure(&config)
        .mount("/", routes![telefork_route])
}
