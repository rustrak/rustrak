//! Counting visitors without cookies: a hash of the project, the visitor's
//! address and browser, and a salt that lives one day. The same person is
//! the same visitor all day; nothing ties them to yesterday, and the address
//! cannot be recovered from the hash.

use rustrak_plugin::prelude::*;
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Deserialize)]
struct Salt {
    salt: String,
}

/// Today's salt (UTC), made the first time it is needed. Older ones are
/// deleted then, so yesterday's hashes can never be recomputed.
fn salt(ctx: &Ctx) -> Result<String> {
    if let Some(salt) = today(ctx)? {
        return Ok(salt);
    }
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| Error::Internal(format!("no randomness: {e}")))?;
    let fresh = hex(&bytes);
    // Two first visitors at once: the first insert wins, both read it back.
    ctx.db().batch(&[
        ("DELETE FROM salts WHERE day <> date('now')", params![]),
        (
            "INSERT OR IGNORE INTO salts (day, salt) VALUES (date('now'), ?1)",
            params![fresh.clone()],
        ),
    ])?;
    today(ctx)?.ok_or_else(|| Error::Internal("today's salt was not kept".into()))
}

fn today(ctx: &Ctx) -> Result<Option<String>> {
    let rows: Vec<Salt> = ctx
        .db()
        .query("SELECT salt FROM salts WHERE day = date('now')", params![])?;
    Ok(rows.into_iter().next().map(|s| s.salt))
}

/// This request's visitor, for today.
pub fn id(ctx: &Ctx, project: u64, user_agent: &str) -> Result<String> {
    let salt = salt(ctx)?;
    let ip = ctx.client_ip().unwrap_or("");
    Ok(hash(&salt, project, ip, user_agent))
}

/// The visitor's hash: short, but still far apart for one site's visitors.
pub fn hash(salt: &str, project: u64, ip: &str, user_agent: &str) -> String {
    let digest = Sha256::new()
        .chain_update(salt)
        .chain_update(project.to_le_bytes())
        .chain_update(ip)
        .chain_update([0])
        .chain_update(user_agent)
        .finalize();
    hex(&digest[..12])
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::hash;

    #[test]
    fn the_same_visitor_on_the_same_day_and_site_is_one() {
        let a = hash("salt", 1, "10.0.0.1", "Firefox");
        assert_eq!(a, hash("salt", 1, "10.0.0.1", "Firefox"));
        assert_eq!(a.len(), 24);
        assert_ne!(a, hash("salt", 1, "10.0.0.2", "Firefox"), "another address");
        assert_ne!(a, hash("salt", 1, "10.0.0.1", "Chrome"), "another browser");
        assert_ne!(a, hash("salt", 2, "10.0.0.1", "Firefox"), "another site");
        assert_ne!(a, hash("tomorrow", 1, "10.0.0.1", "Firefox"), "another day");
    }
}
