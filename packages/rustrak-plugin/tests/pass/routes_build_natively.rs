#![deny(unused)]
// A plugin's own unit tests build it natively: its routes must compile there
// too, the `rustrak:routes` section being the component's alone.
use rustrak_plugin::prelude::*;

mod perm {
    pub const READ: &str = "stats:read";
}
use perm::READ;

// `READ` is only used by `requires`: still a use natively.
#[get("/count", requires = READ)]
fn count() -> NoContent {
    NoContent
}

#[post("/hit", public)]
fn hit() -> NoContent {
    NoContent
}

fn main() {}
