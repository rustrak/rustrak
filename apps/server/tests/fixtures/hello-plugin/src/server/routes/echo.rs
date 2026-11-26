use rustrak_plugin::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
struct Echo {
    message: String,
}

#[post("/echo")]
fn echo(body: Json<Echo>) -> Json<Echo> {
    body
}
