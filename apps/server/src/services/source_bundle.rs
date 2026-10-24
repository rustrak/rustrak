//! Source bundles (`sentry-cli debug-files bundle-jvm` / `bundle-sources`)
//! and the source context read from them, as Symbolicator does.

use std::collections::HashMap;
use std::io::{Cursor, Read as _};

use serde_json::{Map, Value};

/// Symbolicator's `DEFAULT_CONTEXT_LINES`.
const CONTEXT_LINES: usize = 5;

/// The most a source bundle's manifest or one of its source files may expand
/// to before it is read: a small upload must not become a huge allocation.
// ponytail: fixed cap, not from Sentry (Symbolicator reads bundles in its own
// process); make it a config value if real bundles hit it.
pub const MAX_BUNDLE_ENTRY_SIZE: u64 = 16 << 20;

/// symbolic's 8-byte `SYSB` header, then a zip whose `manifest.json` names
/// each file by `url` (JVM bundles) or `path` (native bundles), written as
/// one stream.
pub struct SourceBundle<'a> {
    archive: zip::ZipArchive<Cursor<&'a [u8]>>,
    /// URL or path -> path inside the zip.
    keys: HashMap<String, String>,
    /// Files already read, `None` when unreadable.
    read: HashMap<String, Option<String>>,
}

impl<'a> SourceBundle<'a> {
    pub fn parse(data: &'a [u8]) -> Option<Self> {
        // The zip's offsets count the header, so it opens over the whole
        // stream; the zip crate skips data before the first entry.
        let mut archive = zip::ZipArchive::new(Cursor::new(data))
            .map_err(|e| log::warn!("unreadable source bundle: {}", e))
            .ok()?;
        let manifest = bundle_manifest(&mut archive)?;
        let keys = manifest["files"]
            .as_object()
            .unwrap_or(&Map::new())
            .iter()
            .flat_map(|(zip_path, info)| {
                ["url", "path"]
                    .into_iter()
                    .filter_map(|key| Some((info[key].as_str()?.to_string(), zip_path.clone())))
            })
            .collect();
        Some(Self {
            archive,
            keys,
            read: HashMap::new(),
        })
    }

    /// The file a JVM bundle lists under `url`, or a native bundle under
    /// `path` (its absolute path at build time).
    pub fn source(&mut self, key: &str) -> Option<&str> {
        if !self.read.contains_key(key) {
            let contents = self
                .keys
                .get(key)
                .and_then(|zip_path| read_bundle_entry(&mut self.archive, zip_path));
            self.read.insert(key.to_string(), contents);
        }
        self.read.get(key)?.as_deref()
    }
}

/// A source bundle's `manifest.json`, `None` when missing or unreadable.
pub(crate) fn bundle_manifest<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
) -> Option<Value> {
    serde_json::from_str(&read_bundle_entry(archive, "manifest.json")?).ok()
}

/// One file of a source bundle, `None` when missing, unreadable, or
/// expanding past [`MAX_BUNDLE_ENTRY_SIZE`].
fn read_bundle_entry<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Option<String> {
    let mut out = String::new();
    archive
        .by_name(name)
        .ok()?
        .take(MAX_BUNDLE_ENTRY_SIZE + 1)
        .read_to_string(&mut out)
        .ok()?;
    (out.len() as u64 <= MAX_BUNDLE_ENTRY_SIZE).then_some(out)
}

/// Symbolicator's `get_context_lines` with `trim_to_column` 0.
pub fn get_context_lines(
    source: &str,
    lineno: usize,
) -> Option<(Vec<String>, String, Vec<String>)> {
    let start_line = lineno.saturating_sub(CONTEXT_LINES).saturating_sub(1);
    let line_diff = (lineno - start_line).saturating_sub(1);
    let mut lines = source.lines().skip(start_line);
    let pre = (&mut lines)
        .take(line_diff)
        .map(trim_context_line)
        .collect();
    let line = trim_context_line(lines.next()?);
    let post = lines.take(CONTEXT_LINES).map(trim_context_line).collect();
    Some((pre, line, post))
}

/// Symbolicator's `trim_context_line` at column 0: lines over 150 bytes are
/// cut to their first 140 and marked `{snip}`.
fn trim_context_line(line: &str) -> String {
    let len = line.len();
    if len <= 150 {
        return line.to_string();
    }
    let mut end = 140.min(len);
    if end > len.saturating_sub(5) {
        end = len;
    }
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = line[..end].to_string();
    if end < len {
        out.push_str(" {snip}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_entries_past_the_limit_are_not_read() {
        use std::io::Write as _;
        let limit = MAX_BUNDLE_ENTRY_SIZE as usize;
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, size) in [("fits", limit), ("too-big", limit + 1)] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&vec![b'a'; size]).unwrap();
        }
        let data = zip.finish().unwrap().into_inner();
        let mut archive = zip::ZipArchive::new(Cursor::new(data.as_slice())).unwrap();

        assert_eq!(
            read_bundle_entry(&mut archive, "fits").map(|s| s.len()),
            Some(limit)
        );
        assert_eq!(read_bundle_entry(&mut archive, "too-big"), None);
    }

    #[test]
    fn long_context_lines_are_snipped() {
        let line = "x".repeat(200);
        assert_eq!(
            trim_context_line(&line),
            format!("{} {{snip}}", "x".repeat(140))
        );
    }
}
