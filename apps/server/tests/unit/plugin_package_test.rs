//! `.rtp`: a plugin package is read whole and trusted only once every file
//! matches the hash `MANIFEST` lists for it.

use std::io::Write;

use rustrak::plugins::package::Package;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

const MANIFEST_TOML: &str = r#"
[plugin]
id = "hello"
name = "Hello"
version = "0.1.0"
rustrak = ">=0.1.0"
world = "0.1.0"
"#;

fn base_files() -> Vec<(String, Vec<u8>)> {
    vec![
        (
            "rustrak-plugin.toml".into(),
            MANIFEST_TOML.as_bytes().to_vec(),
        ),
        ("readme/en.md".into(), b"# Hello".to_vec()),
        ("readme/es.md".into(), b"# Hola".to_vec()),
        ("server.wasm".into(), b"\0asm fake".to_vec()),
        (
            "migrations/0001_init.sql".into(),
            b"CREATE TABLE t (x);".to_vec(),
        ),
    ]
}

/// Zips `files` with a MANIFEST of their hashes, then applies `tamper`.
fn rtp(files: &[(String, Vec<u8>)], extra: &[(&str, &[u8])]) -> Vec<u8> {
    let mut manifest: Vec<String> = files
        .iter()
        .map(|(path, bytes)| format!("{}  {path}", hex::encode(Sha256::digest(bytes))))
        .collect();
    manifest.sort();
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (path, bytes) in files {
        zip.start_file(path.as_str(), opts).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.start_file("MANIFEST", opts).unwrap();
    zip.write_all((manifest.join("\n") + "\n").as_bytes())
        .unwrap();
    for (path, bytes) in extra {
        zip.start_file(*path, opts).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    out.into_inner()
}

fn read_err(bytes: &[u8]) -> String {
    match Package::read(bytes) {
        Ok(p) => panic!("should be refused, read {}", p.manifest.plugin.id),
        Err(e) => e.to_string(),
    }
}

#[test]
fn reads_a_well_formed_package() {
    let p = Package::read(&rtp(&base_files(), &[])).expect("should read");
    assert_eq!(p.manifest.plugin.id, "hello");
    assert_eq!(p.wasm, b"\0asm fake");
    assert_eq!(p.readme.get("en").map(String::as_str), Some("# Hello"));
    assert_eq!(p.readme.get("es").map(String::as_str), Some("# Hola"));
    assert_eq!(p.migrations.len(), 1);
    assert_eq!(p.migrations[0].version, 1);
    assert_eq!(p.migrations[0].name, "init");
}

#[test]
fn refuses_a_file_whose_hash_does_not_match() {
    let mut files = base_files();
    let good = rtp(&files, &[]);
    files
        .iter_mut()
        .find(|(p, _)| p == "server.wasm")
        .unwrap()
        .1 = b"\0asm evil".to_vec();
    // Same MANIFEST as the good package, different wasm.
    let manifest = zip::ZipArchive::new(std::io::Cursor::new(good))
        .unwrap()
        .by_name("MANIFEST")
        .map(|mut f| {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut f, &mut s).unwrap();
            s
        })
        .unwrap();
    let mut out = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    let opts = SimpleFileOptions::default();
    for (path, bytes) in &files {
        zip.start_file(path.as_str(), opts).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.start_file("MANIFEST", opts).unwrap();
    zip.write_all(manifest.as_bytes()).unwrap();
    zip.finish().unwrap();
    assert!(read_err(&out.into_inner()).contains("server.wasm"));
}

#[test]
fn refuses_a_file_missing_from_manifest() {
    let bytes = rtp(&base_files(), &[("ui/index.js", b"export default 1")]);
    assert!(read_err(&bytes).contains("ui/index.js"));
}

#[test]
fn refuses_a_path_outside_the_layout() {
    let mut files = base_files();
    files.push(("bin/run.sh".into(), b"rm -rf /".to_vec()));
    assert!(read_err(&rtp(&files, &[])).contains("bin/run.sh"));
}

#[test]
fn refuses_path_traversal() {
    let mut files = base_files();
    files.push(("ui/../../etc/passwd".into(), b"x".to_vec()));
    assert!(read_err(&rtp(&files, &[])).contains(".."));
}

#[test]
fn refuses_a_package_without_readme_or_wasm() {
    for missing in ["readme/en.md", "server.wasm"] {
        let files: Vec<_> = base_files()
            .into_iter()
            .filter(|(p, _)| p != missing)
            .collect();
        assert!(read_err(&rtp(&files, &[])).contains(missing), "{missing}");
    }
}

/// A README per language under `readme/`, English always: the one shown
/// when the reader's language has none.
#[test]
fn a_readme_is_one_per_language_in_readme() {
    let mut files = base_files();
    files.push(("README.md".into(), b"# Root".to_vec()));
    assert!(read_err(&rtp(&files, &[])).contains("README.md"));
    let mut files = base_files();
    files.push(("readme/English.md".into(), b"x".to_vec()));
    assert!(read_err(&rtp(&files, &[])).contains("readme/English.md"));
    let mut files = base_files();
    files.push(("readme/pt-BR.md".into(), "# Olá".as_bytes().to_vec()));
    let p = Package::read(&rtp(&files, &[])).expect("a region is a language too");
    assert!(p.readme.contains_key("pt-BR"));
}

#[test]
fn refuses_a_migration_with_a_bad_name() {
    let mut files = base_files();
    files.push(("migrations/two.sql".into(), b"".to_vec()));
    assert!(read_err(&rtp(&files, &[])).contains("migrations/two.sql"));
}

#[test]
fn refuses_something_that_is_not_a_zip() {
    assert!(!read_err(b"not a zip").is_empty());
}
