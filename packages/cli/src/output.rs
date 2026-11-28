//! How a command prints: a table for a terminal, JSON for a pipe, quiet for a
//! script.
//!
//! The rule is one decision made once. `--json` always wins; otherwise a
//! terminal gets a table and a pipe gets JSON, because a table parsed by
//! `jq` is worthless and a user redirecting to a file usually wants the data.
//! `NO_COLOR` and `TERM=dumb` turn colour off, as everywhere else.

use std::io::IsTerminal;

use comfy_table::{presets, Cell, ContentArrangement, Table};
use serde::Serialize;

use crate::error::{Error, Result};

/// What a command was asked to emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Table,
    Json,
}

#[derive(Debug, Clone, Copy)]
pub struct Output {
    pub format: Format,
    pub colour: bool,
}

impl Output {
    /// Reads the flags once, at the top of a command.
    pub fn resolve(json: bool, table: bool) -> Self {
        let stdout_is_tty = std::io::stdout().is_terminal();
        // `--json` wins; `--table` or a terminal gives a table; a pipe gets
        // JSON, because a table parsed by `jq` is worthless.
        let format = if json {
            Format::Json
        } else if table || stdout_is_tty {
            Format::Table
        } else {
            Format::Json
        };

        let colour = stdout_is_tty
            && std::env::var_os("NO_COLOR").is_none()
            && std::env::var("TERM").map(|t| t != "dumb").unwrap_or(true);

        Self { format, colour }
    }

    pub fn is_json(&self) -> bool {
        self.format == Format::Json
    }

    /// Prints any serializable value as pretty JSON.
    pub fn json<T: Serialize>(&self, value: &T) -> Result<()> {
        let text = serde_json::to_string_pretty(value)
            .map_err(|e| Error::Invalid(format!("could not encode JSON: {e}")))?;
        println!("{text}");
        Ok(())
    }

    /// A table with the given headers, ready for rows.
    pub fn table(&self, headers: &[&str]) -> Table {
        let mut table = Table::new();
        table
            .load_style(if self.colour {
                presets::UTF8_FULL
            } else {
                presets::ASCII_FULL
            })
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(headers.iter().map(|h| Cell::new(*h)));
        table
    }

    /// A line of plain text, obeying `--json` by staying silent: a command
    /// asked for JSON must not also write prose to stdout.
    pub fn line(&self, text: impl AsRef<str>) {
        if !self.is_json() {
            println!("{}", text.as_ref());
        }
    }
}

/// Formats a `date-time` the way a terminal reads it, or passes through
/// whatever the server sent when it is not RFC 3339.
pub fn when(value: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(value) {
        Ok(dt) => dt
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        Err(_) => value.to_string(),
    }
}

/// `2026-11-28T10:00:00Z` -> `3h ago`, the form a triage list wants.
pub fn relative(value: &str) -> String {
    let Ok(then) = chrono::DateTime::parse_from_rfc3339(value) else {
        return value.to_string();
    };
    let seconds = (chrono::Utc::now() - then.with_timezone(&chrono::Utc)).num_seconds();
    ago(seconds)
}

pub fn ago(seconds: i64) -> String {
    let s = seconds.abs();
    let suffix = if seconds < 0 { "from now" } else { "ago" };
    let text = match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m", s / 60),
        3600..=86_399 => format!("{}h", s / 3600),
        86_400..=2_591_999 => format!("{}d", s / 86_400),
        _ => format!("{}mo", s / 2_592_000),
    };
    format!("{text} {suffix}")
}

/// Truncates to `width` display columns, adding an ellipsis, so one long issue
/// title cannot destroy a table's alignment.
pub fn truncate(text: &str, width: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out: String = chars.into_iter().take(width.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Collapses newlines and runs of spaces: a title or a log body is often
/// multiline, and a table row is not.
pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_string_is_untouched() {
        assert_eq!(truncate("abc", 10), "abc");
    }

    #[test]
    fn a_long_string_gets_an_ellipsis() {
        assert_eq!(truncate("abcdefghij", 5), "abcd…");
        assert_eq!(truncate("abcdefghij", 5).chars().count(), 5);
    }

    #[test]
    fn truncating_to_zero_is_empty() {
        assert_eq!(truncate("abcdef", 0), "");
    }

    #[test]
    fn months_use_the_long_unit() {
        assert_eq!(ago(2_592_000), "1mo ago");
        assert_eq!(ago(172_800), "2d ago");
        assert_eq!(ago(7200), "2h ago");
    }

    #[test]
    fn a_title_on_many_lines_becomes_one() {
        assert_eq!(one_line("a\nb\t c "), "a b c");
    }

    #[test]
    fn json_wins_over_a_terminal() {
        let output = Output::resolve(true, false);
        assert_eq!(output.format, Format::Json);
    }
}
