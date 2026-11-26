//! The plugin registry: an index of published plugins, signed with minisign
//! (Ed25519). The index names each package by URL and sha256, so verifying
//! its one signature vouches for every package it lists. What installs from
//! it is verified; a package uploaded by hand is not.

use crate::plugins::package::MAX_PACKAGE_BYTES;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Where the official index lives.
pub const OFFICIAL_INDEX: &str =
    "https://raw.githubusercontent.com/rustrak/plugins/main/registry/index.json";

/// The keys the official index is signed with (minisign public keys, the
/// base64 line). Its secret half signs in rustrak/plugins' CI only.
pub const OFFICIAL_KEYS: &[&str] = &["RWRAGDfsq0nE5fXbtMG+ipN20lntQ9hSTXSGGMFhDMooCJ2uUKAXkPeV"];

/// How long a fetched index is kept before it is fetched again.
const FRESH: Duration = Duration::from_secs(600);

#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("no plugin registry is configured")]
    Off,
    #[error("the registry could not be read: {0}")]
    Unreachable(String),
    #[error("the registry's index signature does not verify: {0}")]
    Signature(String),
    #[error("the registry's index is not valid: {0}")]
    Invalid(String),
    #[error("{0} is not in the registry, or has no version for this Rustrak")]
    NotListed(String),
    #[error("the package downloaded for {0} is not the one the index signed")]
    Tampered(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Index {
    pub plugins: Vec<IndexPlugin>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IndexPlugin {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    /// Its logo, by URL and sha256 like its packages.
    #[serde(default)]
    pub logo: Option<IndexFile>,
    pub versions: Vec<IndexVersion>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IndexFile {
    /// Absolute, or relative to the index.
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IndexVersion {
    pub version: Version,
    /// The Rustrak versions it runs on.
    pub rustrak: VersionReq,
    /// Absolute, or relative to the index.
    pub url: String,
    pub sha256: String,
}

impl IndexPlugin {
    /// Its newest version this Rustrak runs.
    pub fn compatible(&self, host: &Version) -> Option<&IndexVersion> {
        self.versions
            .iter()
            .filter(|v| v.rustrak.matches(host))
            .max_by(|a, b| a.version.cmp(&b.version))
    }
}

/// One published plugin, as the catalog lists it.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RegistryPlugin {
    pub id: String,
    pub name: String,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub description: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub icon: Option<String>,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub author: Option<String>,
    /// Its logo, served by Rustrak once checked against the index.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub logo: Option<String>,
    /// The newest version this Rustrak runs.
    pub version: String,
    /// What is installed now, if any.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub installed_version: Option<String>,
}

/// The registry's plugins, or why there are none.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct RegistryListing {
    pub available: bool,
    #[cfg_attr(feature = "openapi", schema(required))]
    pub error: Option<String>,
    pub plugins: Vec<RegistryPlugin>,
}

/// Checks `index` against its minisign `signature` with any of `keys`.
fn verify(index: &[u8], signature: &str, keys: &[String]) -> Result<Index, RegistryError> {
    let signature = minisign_verify::Signature::decode(signature)
        .map_err(|e| RegistryError::Signature(e.to_string()))?;
    let trusted = keys.iter().any(|key| {
        minisign_verify::PublicKey::from_base64(key)
            .is_ok_and(|pk| pk.verify(index, &signature, false).is_ok())
    });
    if !trusted {
        return Err(RegistryError::Signature(
            "no trusted key signed it".to_string(),
        ));
    }
    serde_json::from_slice(index).map_err(|e| RegistryError::Invalid(e.to_string()))
}

pub struct Registry {
    url: Option<String>,
    keys: Vec<String>,
    http: reqwest::Client,
    cached: Mutex<Option<(Instant, Index)>>,
}

impl Registry {
    pub fn new(url: Option<String>, keys: Vec<String>) -> Self {
        Registry {
            url,
            keys,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("an HTTP client"),
            cached: Mutex::new(None),
        }
    }

    /// The verified index, fetched again once it is ten minutes old.
    pub async fn index(&self) -> Result<Index, RegistryError> {
        let fresh = self
            .cached
            .lock()
            .expect("registry lock")
            .as_ref()
            .filter(|(at, _)| at.elapsed() < FRESH)
            .map(|(_, index)| index.clone());
        if let Some(index) = fresh {
            return Ok(index);
        }
        let url = self.url.as_deref().ok_or(RegistryError::Off)?;
        let bytes = self.fetch(url).await?;
        let signature = self.fetch(&format!("{url}.minisig")).await?;
        let signature =
            String::from_utf8(signature).map_err(|e| RegistryError::Signature(e.to_string()))?;
        let index = verify(&bytes, &signature, &self.keys)?;
        *self.cached.lock().expect("registry lock") = Some((Instant::now(), index.clone()));
        Ok(index)
    }

    /// The package of `id` this Rustrak runs, checked against the index,
    /// with the version the index lists it as.
    pub async fn download(
        &self,
        id: &str,
        host: &Version,
    ) -> Result<(Vec<u8>, Version), RegistryError> {
        let index = self.index().await?;
        let version = index
            .plugins
            .iter()
            .find(|p| p.id == id)
            .and_then(|p| p.compatible(host))
            .ok_or_else(|| RegistryError::NotListed(id.to_string()))?;
        let bytes = self
            .fetch_checked(id, &version.url, &version.sha256)
            .await?;
        Ok((bytes, version.version.clone()))
    }

    /// The logo of `id`, checked against the index like a package.
    pub async fn logo(&self, id: &str) -> Result<Vec<u8>, RegistryError> {
        let index = self.index().await?;
        let logo = index
            .plugins
            .iter()
            .find(|p| p.id == id)
            .and_then(|p| p.logo.clone())
            .ok_or_else(|| RegistryError::NotListed(id.to_string()))?;
        self.fetch_checked(id, &logo.url, &logo.sha256).await
    }

    /// A file the signed index lists, refused unless it is the one listed.
    async fn fetch_checked(
        &self,
        id: &str,
        url: &str,
        sha256: &str,
    ) -> Result<Vec<u8>, RegistryError> {
        let bytes = self.fetch(&self.resolve(url)).await?;
        if !hex::encode(Sha256::digest(&bytes)).eq_ignore_ascii_case(sha256) {
            return Err(RegistryError::Tampered(id.to_string()));
        }
        Ok(bytes)
    }

    /// A file's URL, relative to the index's.
    fn resolve(&self, url: &str) -> String {
        if url.contains("://") {
            return url.to_string();
        }
        let base = self.url.as_deref().unwrap_or_default();
        let dir = base.rsplit_once('/').map_or(base, |(dir, _)| dir);
        format!("{dir}/{url}")
    }

    async fn fetch(&self, url: &str) -> Result<Vec<u8>, RegistryError> {
        let too_big = || RegistryError::Unreachable(format!("{url} is bigger than any package"));
        if let Some(path) = url.strip_prefix("file://") {
            use tokio::io::AsyncReadExt;
            let unreachable =
                |e: std::io::Error| RegistryError::Unreachable(format!("{path}: {e}"));
            let mut bytes = Vec::new();
            tokio::fs::File::open(path)
                .await
                .map_err(unreachable)?
                .take(MAX_PACKAGE_BYTES + 1)
                .read_to_end(&mut bytes)
                .await
                .map_err(unreachable)?;
            if bytes.len() as u64 > MAX_PACKAGE_BYTES {
                return Err(too_big());
            }
            return Ok(bytes);
        }
        let mut response = self
            .http
            .get(url)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|e| RegistryError::Unreachable(e.to_string()))?;
        // Nothing the registry serves is bigger than a package: stop there,
        // before a wrong or hostile server fills the memory.
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| RegistryError::Unreachable(e.to_string()))?
        {
            bytes.extend_from_slice(&chunk);
            if bytes.len() as u64 > MAX_PACKAGE_BYTES {
                return Err(too_big());
            }
        }
        Ok(bytes)
    }
}
