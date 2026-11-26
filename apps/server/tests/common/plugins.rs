//! The test plugins, built by `rustrak-plugin build` exactly as their
//! authors build them, once per test binary: the `hello` fixture
//! (`tests/fixtures/hello-plugin`) and `plugins/web-traffic`.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use rustrak::plugins::package::Package;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

pub fn hello_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/hello-plugin")
}

/// Builds the plugin project in `dir` with the authors' CLI; its package.
fn build(dir: &Path) -> Vec<u8> {
    let built = Command::new("node")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/plugin/src/cli.ts"))
        .arg("build")
        .current_dir(dir)
        // Its own target dir, not the server's: the two never share a lock.
        .env_remove("CARGO_TARGET_DIR")
        .status()
        .expect("node should run (pnpm install first)");
    assert!(built.success(), "{} failed to build", dir.display());
    let pkg: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.join("package.json")).unwrap()).unwrap();
    let built = format!("-{}.rtp", pkg["version"].as_str().unwrap());
    let rtp = std::fs::read_dir(dir.join("dist"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.to_string_lossy().ends_with(&built))
        .expect("the package it built");
    std::fs::read(rtp).unwrap()
}

fn hello_built() -> &'static [u8] {
    static RTP: OnceLock<Vec<u8>> = OnceLock::new();
    RTP.get_or_init(|| build(&hello_dir()))
}

/// The files of a package, its `MANIFEST` left out.
pub fn files_of(rtp: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(rtp)).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut file = zip.by_index(i).unwrap();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            (file.name().unwrap().to_string(), bytes)
        })
        .filter(|(path, _)| path != "MANIFEST")
        .collect()
}

/// `hello`'s compiled component.
pub fn hello_wasm() -> &'static [u8] {
    static WASM: OnceLock<Vec<u8>> = OnceLock::new();
    WASM.get_or_init(|| Package::read(hello_built()).unwrap().wasm)
}

/// `hello` packed, with its manifest edited by `edit`.
pub fn hello_rtp_with(edit: impl Fn(String) -> String) -> Vec<u8> {
    let files: Vec<_> = files_of(hello_built())
        .into_iter()
        .map(|(path, bytes)| match path.as_str() {
            "rustrak-plugin.toml" => {
                let manifest = edit(String::from_utf8(bytes).unwrap());
                (path, manifest.into_bytes())
            }
            _ => (path, bytes),
        })
        .collect();
    zip_with_manifest(&files)
}

pub fn hello_rtp() -> Vec<u8> {
    hello_built().to_vec()
}

/// Zips `files` with the `MANIFEST` of their sha256 hashes.
pub fn zip_with_manifest(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut listing: Vec<String> = files
        .iter()
        .map(|(path, bytes)| format!("{}  {path}", hex::encode(Sha256::digest(bytes))))
        .collect();
    listing.sort();
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    let opts = SimpleFileOptions::default();
    for (path, bytes) in files {
        zip.start_file(path.as_str(), opts).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.start_file("MANIFEST", opts).unwrap();
    zip.write_all((listing.join("\n") + "\n").as_bytes())
        .unwrap();
    zip.finish().unwrap();
    out.into_inner()
}

/// `hello`'s migrations, as its package carries them.
pub fn hello_migrations() -> Vec<rustrak::plugins::package::Migration> {
    Package::read(hello_built()).unwrap().migrations
}

/// What `hello`'s settings interface answers with nothing set: its defaults.
pub fn hello_settings() -> rustrak::plugins::runtime::SettingsSource {
    let specs = Package::read(hello_built()).unwrap().manifest.settings;
    let defaults = serde_json::Value::Object(rustrak::plugins::settings::resolve(
        &specs,
        &serde_json::Map::new(),
        None,
    ))
    .to_string();
    std::sync::Arc::new(move |_| defaults.clone())
}
