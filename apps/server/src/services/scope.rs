//! What a page's filters narrow a read to: the project overview's
//! environment and release, applied the same way to events, issues and
//! sessions so every panel answers for the same slice.

/// Both unset is the whole project.
#[derive(Debug, Default, Clone, Copy)]
pub struct Scope<'a> {
    /// Only what happened in this environment.
    pub environment: Option<&'a str>,
    /// Only what happened in this release.
    pub release: Option<&'a str>,
}

impl<'a> Scope<'a> {
    pub fn environment(environment: Option<&'a str>) -> Self {
        Self {
            environment,
            release: None,
        }
    }
}

/// Bucket width for a window, in hours: about 24 to 60 buckets for any of
/// the dashboard's windows.
pub(crate) fn bucket_hours(period_hours: i64) -> i64 {
    match period_hours {
        ..=24 => 1,
        25..=168 => 3,
        169..=336 => 6,
        337..=720 => 12,
        _ => 24,
    }
}
