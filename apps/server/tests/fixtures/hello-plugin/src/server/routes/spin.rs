use rustrak_plugin::prelude::*;

/// Never returns: proves the host stops a runaway plugin.
#[get("/spin", instance)]
fn spin() -> NoContent {
    loop {
        std::hint::spin_loop();
    }
}

/// Sleeps an hour: proves a wait cannot outlive the call's budget either.
#[get("/sleep", instance)]
fn sleep() -> NoContent {
    std::thread::sleep(std::time::Duration::from_secs(3600));
    NoContent
}

/// Sleeps well within its budget, which it is allowed to.
#[get("/nap", instance)]
fn nap() -> NoContent {
    std::thread::sleep(std::time::Duration::from_millis(10));
    NoContent
}
