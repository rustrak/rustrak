//! Password login attempts per email: ten a minute, as Sentry limits
//! `auth:login:username` (`src/sentry/api/endpoints/auth_login.py`). Every
//! attempt counts, right or wrong, and an unknown email counts like a known
//! one, so the limit itself says nothing about which accounts exist.
//!
//! In memory because one process owns the database. Keyed by the SHA-256 of
//! the normalized email (Sentry keys by its md5), so an entry is the same size
//! whatever the caller sends. Expired entries are swept once a window, so none
//! outlives two; a new email costs an Argon2 run, which keeps the map small.

use sha2::{Digest as _, Sha256};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const LIMIT: u32 = 10;
const WINDOW: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct LoginLimiter {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    attempts: HashMap<[u8; 32], (Instant, u32)>,
    last_sweep: Option<Instant>,
}

impl LoginLimiter {
    /// Records one attempt for `email`. `Err` holds the seconds until the
    /// next attempt is allowed.
    pub fn attempt(&self, email: &str) -> Result<(), u64> {
        self.attempt_at(email, Instant::now())
    }

    fn attempt_at(&self, email: &str, now: Instant) -> Result<(), u64> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let State {
            attempts,
            last_sweep,
        } = &mut *state;
        let sweep_due = last_sweep.is_none_or(|at| now.duration_since(at) >= WINDOW);
        if sweep_due {
            attempts.retain(|_, (start, _)| now.duration_since(*start) < WINDOW);
            *last_sweep = Some(now);
        }
        let key = Sha256::digest(crate::models::User::normalize_email(email)).into();
        let (start, count) = attempts.entry(key).or_insert((now, 0));
        if now.duration_since(*start) >= WINDOW {
            (*start, *count) = (now, 0);
        }
        *count += 1;
        if *count <= LIMIT {
            return Ok(());
        }
        let left = WINDOW.saturating_sub(now.duration_since(*start));
        Err(left.as_secs_f64().ceil().max(1.0) as u64)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.state.lock().unwrap().attempts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_resets_after_a_minute() {
        let limiter = LoginLimiter::default();
        let t0 = Instant::now();
        for _ in 0..LIMIT {
            assert!(limiter.attempt_at("a@example.com", t0).is_ok());
        }
        assert_eq!(limiter.attempt_at("a@example.com", t0), Err(60));
        assert_eq!(
            limiter.attempt_at("a@example.com", t0 + Duration::from_secs(45)),
            Err(15)
        );
        assert!(limiter.attempt_at("a@example.com", t0 + WINDOW).is_ok());
    }

    #[test]
    fn expired_entries_are_swept_once_a_window() {
        let limiter = LoginLimiter::default();
        let t0 = Instant::now();
        limiter.attempt_at("early@example.com", t0).unwrap();
        limiter.attempt_at("late@example.com", t0 + WINDOW).unwrap();
        assert_eq!(limiter.len(), 1);
    }
}
