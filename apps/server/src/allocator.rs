//! mimalloc's defaults trade memory for speed: freed pages go back to the OS
//! a second later, and on Linux its regions are backed by 2 MiB transparent
//! huge pages, so a few live bytes keep a whole huge page resident. Returning
//! freed memory at once and not using huge pages cuts resident memory several
//! times over at the same throughput.

use std::ffi::{c_int, c_long};

// The global allocator is declared in main.rs; this keeps mimalloc linked
// into the library, whose tests call it too.
use mimalloc as _;

// `mi_option_t` values in mimalloc v3's `mimalloc.h`.
const PURGE_DELAY: c_int = 15;
const ALLOW_THP: c_int = 43;

extern "C" {
    #[cfg(any(target_os = "linux", test))]
    fn mi_option_get(option: c_int) -> c_long;
    fn mi_option_set_default(option: c_int, value: c_long);
}

/// Makes freed memory go back to the OS at once and turns transparent huge
/// pages off for the process. `MIMALLOC_PURGE_DELAY` and `MIMALLOC_ALLOW_THP`
/// still override each. Call it first thing in `main`, before any thread.
pub fn tune() {
    // SAFETY: plain setters on mimalloc's option table; a default does not
    // replace a value the environment set.
    unsafe {
        mi_option_set_default(PURGE_DELAY, 0);
        mi_option_set_default(ALLOW_THP, 0);
    }
    // mimalloc only calls this itself when the environment turns THP off,
    // before `main`: without it a kernel in `always` mode still backs the
    // heap with huge pages.
    #[cfg(target_os = "linux")]
    if option(ALLOW_THP) == 0 {
        let _ = nix::sys::prctl::set_thp_disable(true);
    }
}

#[cfg(any(target_os = "linux", test))]
fn option(option: c_int) -> c_long {
    // SAFETY: reads an option; `option` is in range.
    unsafe { mi_option_get(option) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tune_purges_at_once_and_turns_huge_pages_off() {
        // mimalloc's own defaults, which also proves the option numbers.
        assert_eq!((option(PURGE_DELAY), option(ALLOW_THP)), (1000, 1));
        tune();
        assert_eq!((option(PURGE_DELAY), option(ALLOW_THP)), (0, 0));
    }
}
