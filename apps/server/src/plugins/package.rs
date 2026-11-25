//! `.rtp`: a plugin package, a zip with a fixed layout and a `MANIFEST` of
//! sha256 hashes. Nothing in it is used until every file matches its hash.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use sha2::{Digest, Sha256};

use super::manifest::Manifest;

pub const MAX_PACKAGE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_WASM_BYTES: usize = 5 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct PackageError(String);

fn fail<T>(msg: impl Into<String>) -> Result<T, PackageError> {
    Err(PackageError(msg.into()))
}

/// One forward-only migration, `migrations/NNNN_name.sql`.
#[derive(Debug, Clone)]
pub struct Migration {
    pub version: u32,
    pub name: String,
    pub sql: String,
    /// sha256 of `sql`, to notice a published migration that was edited.
    pub checksum: String,
}

#[derive(Debug)]
pub struct Package {
    pub manifest: Manifest,
    pub wasm: Vec<u8>,
    /// Its page in the catalog, Markdown, by language (`readme/<locale>.md`);
    /// English always, shown when the reader's language has none.
    pub readme: BTreeMap<String, String>,
    pub migrations: Vec<Migration>,
    /// Every verified file by path, `MANIFEST` and its signature included.
    pub files: BTreeMap<String, Vec<u8>>,
}

const TOP_LEVEL: &[&str] = &[
    "rustrak-plugin.toml",
    "CHANGELOG.md",
    "logo.svg",
    "server.wasm",
    "BUILD",
    "MANIFEST",
    "MANIFEST.minisig",
];
const DIRS: &[&str] = &[
    "readme/",
    "ui/",
    "public/",
    "messages/",
    "migrations/",
    "screenshots/",
    "schemas/",
];

fn allowed(path: &str) -> bool {
    TOP_LEVEL.contains(&path)
        || DIRS
            .iter()
            .any(|d| path.len() > d.len() && path.starts_with(d))
}

/// `en`, `es`, `pt-BR`: a language, with its region if it has one.
fn is_locale(tag: &str) -> bool {
    let mut parts = tag.split('-');
    let lang = parts.next().unwrap_or("");
    let region = parts.next();
    parts.next().is_none()
        && (2..=3).contains(&lang.len())
        && lang.bytes().all(|b| b.is_ascii_lowercase())
        && region.is_none_or(|r| r.len() == 2 && r.bytes().all(|b| b.is_ascii_uppercase()))
}

impl Package {
    pub fn read(bytes: &[u8]) -> Result<Self, PackageError> {
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| PackageError(format!("not a plugin package: {e}")))?;

        let mut files = BTreeMap::new();
        let mut total = 0u64;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| PackageError(e.to_string()))?;
            if entry.is_dir() {
                continue;
            }
            let path = entry
                .name()
                .map_err(|e| PackageError(format!("an entry name is not valid: {e}")))?
                .into_owned();
            if path.starts_with('/')
                || path.contains('\\')
                || path.split('/').any(|c| c == ".." || c.is_empty())
            {
                return fail(format!(
                    "{path:?}: paths are relative, without .. or empty parts"
                ));
            }
            if entry.is_symlink() {
                return fail(format!("{path}: symbolic links are not allowed"));
            }
            if !allowed(&path) {
                return fail(format!("{path} is not part of a plugin package"));
            }
            let mut buf = Vec::new();
            // Read one byte past the budget so a lying header cannot sneak by.
            (&mut entry)
                .take(MAX_PACKAGE_BYTES - total + 1)
                .read_to_end(&mut buf)
                .map_err(|e| PackageError(format!("{path}: {e}")))?;
            total += buf.len() as u64;
            if total > MAX_PACKAGE_BYTES {
                return fail(format!(
                    "the package is larger than {} MB",
                    MAX_PACKAGE_BYTES >> 20
                ));
            }
            files.insert(path, buf);
        }

        verify_hashes(&files)?;

        let text = |path: &str| -> Result<String, PackageError> {
            let bytes = files
                .get(path)
                .ok_or_else(|| PackageError(format!("{path} is missing")))?;
            String::from_utf8(bytes.clone())
                .map_err(|_| PackageError(format!("{path} is not UTF-8")))
        };
        let manifest = Manifest::parse(&text("rustrak-plugin.toml")?)
            .map_err(|e| PackageError(format!("rustrak-plugin.toml: {e}")))?;
        let mut readme = BTreeMap::new();
        for path in files.keys().filter(|p| p.starts_with("readme/")) {
            let locale = path
                .strip_prefix("readme/")
                .and_then(|name| name.strip_suffix(".md"))
                .filter(|l| is_locale(l))
                .ok_or_else(|| {
                    PackageError(format!(
                        "{path}: a README is readme/<language>.md, like readme/en.md"
                    ))
                })?;
            readme.insert(locale.to_string(), text(path)?);
        }
        if !readme.contains_key("en") {
            return fail(
                "readme/en.md is missing: English is the README every reader can fall back to",
            );
        }
        let wasm = files
            .get("server.wasm")
            .cloned()
            .ok_or_else(|| PackageError("server.wasm is missing".into()))?;
        if wasm.len() > MAX_WASM_BYTES {
            return fail(format!(
                "server.wasm is larger than {} MB",
                MAX_WASM_BYTES >> 20
            ));
        }
        let migrations = migrations(&files)?;
        if manifest.ui.as_ref().is_some_and(|ui| !ui.is_empty())
            && !files.contains_key("ui/index.js")
        {
            return fail("the manifest declares pages or widgets but ui/index.js is missing");
        }

        Ok(Package {
            manifest,
            wasm,
            readme,
            migrations,
            files,
        })
    }
}

/// Every file but `MANIFEST` and its signature is listed, with its sha256.
fn verify_hashes(files: &BTreeMap<String, Vec<u8>>) -> Result<(), PackageError> {
    let listing = files
        .get("MANIFEST")
        .and_then(|b| std::str::from_utf8(b).ok())
        .ok_or_else(|| PackageError("MANIFEST is missing".into()))?;
    let mut listed = BTreeMap::new();
    for line in listing.lines().filter(|l| !l.is_empty()) {
        let (hash, path) = line
            .split_once("  ")
            .ok_or_else(|| PackageError(format!("MANIFEST: malformed line {line:?}")))?;
        listed.insert(path, hash);
    }
    for (path, bytes) in files {
        if path == "MANIFEST" || path == "MANIFEST.minisig" {
            continue;
        }
        match listed.remove(path.as_str()) {
            Some(hash) if hash == hex::encode(Sha256::digest(bytes)) => {}
            Some(_) => return fail(format!("{path} does not match its hash in MANIFEST")),
            None => return fail(format!("{path} is not listed in MANIFEST")),
        }
    }
    if let Some(path) = listed.keys().next() {
        return fail(format!("{path} is listed in MANIFEST but missing"));
    }
    Ok(())
}

fn migrations(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<Migration>, PackageError> {
    let mut out = Vec::new();
    for (path, bytes) in files.range("migrations/".to_string()..) {
        let Some(file) = path.strip_prefix("migrations/") else {
            break;
        };
        let parsed = file
            .strip_suffix(".sql")
            .and_then(|stem| stem.split_once('_'))
            .filter(|(n, name)| n.len() == 4 && !name.is_empty())
            .and_then(|(n, name)| Some((n.parse::<u32>().ok()?, name)));
        let Some((version, name)) = parsed else {
            return fail(format!("{path}: migrations are named NNNN_name.sql"));
        };
        let sql = String::from_utf8(bytes.clone())
            .map_err(|_| PackageError(format!("{path} is not UTF-8")))?;
        out.push(Migration {
            version,
            name: name.to_string(),
            checksum: hex::encode(Sha256::digest(&sql)),
            sql,
        });
    }
    // BTreeMap order is already by version; equal numbers are a mistake.
    if let Some(w) = out.windows(2).find(|w| w[0].version == w[1].version) {
        return fail(format!("two migrations share number {:04}", w[0].version));
    }
    Ok(out)
}
