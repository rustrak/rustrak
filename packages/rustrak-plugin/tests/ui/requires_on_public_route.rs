use rustrak_plugin::prelude::*;

#[post("/hit", public, requires = "stats:read")]
fn hit() -> NoContent {
    NoContent
}

fn main() {}
