//! Where a profile lives and what it holds.
//!
//! One file, `~/.config/rustrak/config.toml` (or the platform's config
//! directory), with a `current` profile and one table per profile. The token is
//! in the same file as the URL because they are useless apart: a token is only
//! valid against the instance that issued it. It is written `0600` on unix.
//!
//! Every field can be overridden per invocation, and the environment wins over
//! the file so CI can run the CLI with `RUSTRAK_URL` and `RUSTRAK_TOKEN` and no
//! config at all.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ConfigFile {
    /// The profile commands use when none is named.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<String>,
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Profile {
    /// The instance, without a trailing slash.
    pub url: String,
    /// The bearer token minted by `auth login`. Optional so a profile can be
    /// created before its owner signs in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// The address that signed in. Recorded at login because nothing on the
    /// bearer path reports it: `/auth/me` is session-only, so `whoami` would
    /// otherwise have to ask the dashboard. Absent for a `--token` login.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
}

/// What a command actually talks to, once flags, environment and file are
/// reconciled.
#[derive(Debug, Clone)]
pub struct Resolved {
    pub profile: Option<String>,
    pub url: String,
    pub token: Option<String>,
}

impl ConfigFile {
    pub fn path() -> Result<PathBuf> {
        if let Some(path) = std::env::var_os("RUSTRAK_CONFIG") {
            return Ok(PathBuf::from(path));
        }
        let dir = dirs::config_dir()
            .ok_or_else(|| Error::Config("could not locate a config directory".to_string()))?;
        Ok(dir.join("rustrak").join("config.toml"))
    }

    pub fn load() -> Result<Self> {
        Self::load_from(&Self::path()?)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|e| Error::Config(format!("{path:?}: {e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(Error::Config(format!("{path:?}: {e}"))),
        }
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::path()?)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)
            .map_err(|e| Error::Config(format!("could not serialize config: {e}")))?;
        // Write beside the target and rename, so an interrupted save cannot
        // truncate a working config, and set the mode before the content is
        // ever readable there.
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, &text)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// The profile a command uses: the one it named, else `current`, else the
    /// only one there is. `None` when the file holds no profiles at all.
    pub fn select(&self, named: Option<&str>) -> Result<Option<(String, Profile)>> {
        if let Some(name) = named {
            let profile = self.profiles.get(name).ok_or_else(|| {
                Error::Config(format!(
                    "no profile named {name:?}; run `rustrak config list` to see them"
                ))
            })?;
            return Ok(Some((name.to_string(), profile.clone())));
        }
        if let Some(name) = &self.current {
            if let Some(profile) = self.profiles.get(name) {
                return Ok(Some((name.clone(), profile.clone())));
            }
        }
        if self.profiles.len() == 1 {
            let (name, profile) = self.profiles.iter().next().expect("one profile");
            return Ok(Some((name.clone(), profile.clone())));
        }
        Ok(None)
    }

    /// Writes (or replaces) a profile, and makes it current.
    pub fn upsert(&mut self, name: &str, profile: Profile) {
        self.profiles.insert(name.to_string(), profile);
        self.current = Some(name.to_string());
    }
}

/// Reconciles the flag, the environment and the config file into one target.
///
/// Precedence is flag, then environment, then the selected profile; a URL with
/// no token is a valid target (`health` needs none), a token with no URL is
/// not.
pub fn resolve(
    named_profile: Option<&str>,
    url_flag: Option<&str>,
    token_flag: Option<&str>,
) -> Result<Resolved> {
    let config = ConfigFile::load()?;
    let profile_env = std::env::var("RUSTRAK_PROFILE").ok();
    let named = named_profile.or(profile_env.as_deref());
    let selected = config.select(named)?;

    let url = url_flag
        .map(str::to_string)
        .or_else(|| std::env::var("RUSTRAK_URL").ok())
        .or_else(|| selected.as_ref().map(|(_, p)| p.url.clone()))
        .filter(|u| !u.is_empty())
        .ok_or_else(|| {
            Error::Config(
                "no instance configured; run `rustrak auth login --url <instance>` or set RUSTRAK_URL"
                    .to_string(),
            )
        })?;

    let token = token_flag
        .map(str::to_string)
        .or_else(|| std::env::var("RUSTRAK_TOKEN").ok())
        .filter(|t| !t.is_empty())
        .or_else(|| selected.as_ref().and_then(|(_, p)| p.token.clone()));

    Ok(Resolved {
        profile: selected.map(|(name, _)| name),
        url: normalise_url(&url),
        token,
    })
}

/// Trims a trailing slash and validates the scheme, so `--url` pasted with one
/// still joins correctly and a bare host gets a clear error rather than a
/// confusing parse failure deep in the client.
pub fn normalise_url(url: &str) -> String {
    let trimmed = url.trim().trim_end_matches('/');
    if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(profiles: &[(&str, &str, Option<&str>)], current: Option<&str>) -> ConfigFile {
        let mut config = ConfigFile {
            current: current.map(str::to_string),
            profiles: BTreeMap::new(),
        };
        for (name, url, token) in profiles {
            config.profiles.insert(
                name.to_string(),
                Profile {
                    url: url.to_string(),
                    token: token.map(str::to_string),
                    email: None,
                },
            );
        }
        config
    }

    #[test]
    fn a_named_profile_wins_over_current() {
        let config = file(
            &[("a", "https://a", None), ("b", "https://b", None)],
            Some("a"),
        );
        let (name, profile) = config.select(Some("b")).unwrap().unwrap();
        assert_eq!(name, "b");
        assert_eq!(profile.url, "https://b");
    }

    #[test]
    fn current_is_the_default() {
        let config = file(
            &[("a", "https://a", None), ("b", "https://b", None)],
            Some("b"),
        );
        assert_eq!(config.select(None).unwrap().unwrap().0, "b");
    }

    #[test]
    fn a_single_profile_needs_no_current() {
        let config = file(&[("only", "https://only", None)], None);
        assert_eq!(config.select(None).unwrap().unwrap().0, "only");
    }

    #[test]
    fn two_profiles_and_no_current_is_ambiguous() {
        let config = file(&[("a", "https://a", None), ("b", "https://b", None)], None);
        assert!(config.select(None).unwrap().is_none());
    }

    #[test]
    fn an_unknown_profile_is_an_error() {
        let config = file(&[("a", "https://a", None)], Some("a"));
        assert!(config.select(Some("nope")).is_err());
    }

    #[test]
    fn a_url_without_a_scheme_gets_https() {
        assert_eq!(
            normalise_url("rustrak.example.com"),
            "https://rustrak.example.com"
        );
    }

    #[test]
    fn a_trailing_slash_is_trimmed() {
        assert_eq!(
            normalise_url("http://localhost:8080/"),
            "http://localhost:8080"
        );
    }

    #[test]
    fn a_round_trip_through_toml_keeps_profiles() {
        let config = file(
            &[("default", "https://r.example", Some("abc"))],
            Some("default"),
        );
        let text = toml::to_string_pretty(&config).unwrap();
        let back: ConfigFile = toml::from_str(&text).unwrap();
        assert_eq!(back.current.as_deref(), Some("default"));
        assert_eq!(back.profiles["default"].token.as_deref(), Some("abc"));
    }
}
