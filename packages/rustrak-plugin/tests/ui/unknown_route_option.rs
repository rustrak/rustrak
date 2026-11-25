use rustrak_plugin::prelude::*;

#[get("/x", pubic)]
fn x() -> NoContent {
    NoContent
}

fn main() {}
