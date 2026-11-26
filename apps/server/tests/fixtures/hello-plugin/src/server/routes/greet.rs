use rustrak_plugin::prelude::*;
use serde::Deserialize;

#[derive(Deserialize)]
struct Who {
    name: String,
}

#[get("/greet")]
fn greet(who: Query<Who>) -> String {
    format!("hello {}", who.0.name)
}
