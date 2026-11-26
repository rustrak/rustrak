use rustrak_plugin::prelude::*;

/// Never returns: proves the host stops a runaway plugin.
#[get("/spin", instance)]
fn spin() -> NoContent {
    loop {
        std::hint::spin_loop();
    }
}
