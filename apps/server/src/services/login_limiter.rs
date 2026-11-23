//! Password login attempts per email: ten a minute, as Sentry limits
//! `auth:login:username` (`src/sentry/api/endpoints/auth_login.py`). Every
//! attempt counts, right or wrong, and an unknown email counts like a known
//! one, so the limit itself says nothing about which accounts exist.
//!
//! In memory because one process owns the database. An entry lives one
//! window; a new email costs an Argon2 run, which keeps the map small.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const LIMIT: u32 = 10;
const WINDOW: Duration = Duration::from_secs(60);
/// Expired entries are swept once the map holds this many.
const SWEEP_AT: usize = 1024;

#[derive(Default)]
pub struct LoginLimiter {
    attempts: Mutex<HashMap<String, (Instant, u32)>>,
}

impl LoginLimiter {
    /// Records one attempt for `email`. `Err` holds the seconds until the
    /// next attempt is allowed.
    pub fn attempt(&self, email: &str) -> Result<(), u64> {
        self.attempt_at(email, Instant::now())
    }

    fn attempt_at(&self, email: &str, now: Instant) -> Result<(), u64> {
        let mut attempts = self.attempts.lock().unwrap_or_else(|e| e.into_inner());
        if attempts.len() >= SWEEP_AT {
            attempts.retain(|_, (start, _)| now.duration_since(*start) < WINDOW);
        }
        let (start, count) = attempts
            .entry(crate::models::User::normalize_email(email))
            .or_insert((now, 0));
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
    fn expired_entries_are_swept() {
        let limiter = LoginLimiter::default();
        let t0 = Instant::now();
        for i in 0..SWEEP_AT {
            limiter.attempt_at(&format!("{i}@example.com"), t0).unwrap();
        }
        limiter.attempt_at("late@example.com", t0 + WINDOW).unwrap();
        assert_eq!(limiter.attempts.lock().unwrap().len(), 1);
    }
}
