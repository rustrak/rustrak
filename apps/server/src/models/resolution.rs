//! Release order and issue resolutions: Sentry's `GroupResolution`
//! (`src/sentry/models/groupresolution.py`) and the release helpers it
//! leans on (`follows_semver_versioning_scheme`, `greatest_semver_release`,
//! `get_next_release`), as pure functions over a project's releases.
//!
//! Versions compare with `sentry-release-parser`, the crate behind Relay's
//! `relay_compare_versions`, which is what Sentry's `compare_version` calls.

use chrono::{DateTime, Utc};
use sentry_release_parser::{Release, Version};
use std::cmp::Ordering;

/// `GroupResolution.Type`, stored as Sentry stores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionType {
    /// Fixed in a known release: events from it or later regress.
    InRelease,
    /// Fixed in whatever release ships next.
    InNextRelease,
}

impl ResolutionType {
    pub const fn code(self) -> i16 {
        match self {
            Self::InRelease => 0,
            Self::InNextRelease => 1,
        }
    }

    pub fn from_code(code: Option<i16>) -> Option<Self> {
        match code? {
            0 => Some(Self::InRelease),
            1 => Some(Self::InNextRelease),
            _ => None,
        }
    }
}

/// `GroupResolution.Status`.
pub const STATUS_PENDING: i16 = 0;
pub const STATUS_RESOLVED: i16 = 1;

/// The columns of a release the ordering reads.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ReleaseRef {
    pub id: i32,
    pub version: String,
    /// Sentry's `date_added`: when the release was first registered.
    pub date_created: DateTime<Utc>,
    pub date_released: Option<DateTime<Utc>>,
}

impl ReleaseRef {
    /// `COALESCE(date_released, date_added)`, the "most recent" order.
    fn recency(&self) -> DateTime<Utc> {
        self.date_released.unwrap_or(self.date_created)
    }
}

/// An issue's resolution with the releases it names.
#[derive(Debug, Clone, Copy)]
pub struct ResolutionRef<'a> {
    /// `None` for a row Sentry wrote before types existed.
    pub kind: Option<ResolutionType>,
    pub release: &'a ReleaseRef,
    pub current_release_version: Option<&'a str>,
    /// The release row `current_release_version` names, if it still exists.
    pub current_release: Option<&'a ReleaseRef>,
}

/// `validate_bigint`: a version component Postgres' `bigint` can hold.
fn fits_bigint(n: u64) -> bool {
    n <= i64::MAX as u64
}

/// The parsed version of a `package@version` release, as Sentry stores it in
/// the release's semver columns.
fn semver_of(version: &str) -> Option<Version<'_>> {
    let release = Release::parse(version).ok()?;
    release.package()?;
    let parsed = release.version()?;
    let (major, minor, patch, revision) = parsed.quad();
    [major, minor, patch, revision]
        .into_iter()
        .all(fits_bigint)
        .then(|| parsed.clone())
}

/// `Release.is_semver_version`: a valid `package@version` whose version
/// parses and fits the semver columns.
pub fn is_semver_version(version: &str) -> bool {
    crate::models::is_valid_version(version) && semver_of(version).is_some()
}

/// Sentry's `compare_version(parse_release(a).version_raw,
/// parse_release(b).version_raw)`: `None` where Sentry raises `RelayError`.
pub fn compare_versions(a: &str, b: &str) -> Option<Ordering> {
    let a = Release::parse(a).ok()?;
    let b = Release::parse(b).ok()?;
    let a = Version::parse(a.version_raw()).ok()?;
    let b = Version::parse(b.version_raw()).ok()?;
    Some(a.cmp(&b))
}

/// `GroupResolution.has_resolution`: whether `resolution` still covers an
/// event from `release`, so the event is not a regression. `follows_semver`
/// is the project's [`ProjectReleases::follows_semver`] for `release`.
///
/// Sentry's legacy order (`date_added`), not the
/// `release-resolution-finalized-order` flag.
pub fn has_resolution(
    resolution: Option<&ResolutionRef<'_>>,
    release: Option<&ReleaseRef>,
    follows_semver: bool,
) -> bool {
    let Some(res) = resolution else {
        return false;
    };
    // No release: the app went from "no release" to "some release", so
    // this event must be older.
    let Some(release) = release else {
        return true;
    };

    // Resolved in next release: covered up to the release that was current.
    if let Some(current) = res.current_release_version.filter(|v| !v.is_empty()) {
        if follows_semver {
            if let Some(order) = compare_versions(current, &release.version) {
                if order != Ordering::Less {
                    return true;
                }
                // A current version recorded under another ordering may be
                // stale: an event older than the fix is still covered.
                if let Some(order) = compare_versions(&res.release.version, &release.version) {
                    return order == Ordering::Greater;
                }
            }
        } else if let Some(current) = res.current_release {
            return current.id == release.id || current.date_created > release.date_created;
        }
    }

    match res.kind {
        None | Some(ResolutionType::InNextRelease) => {
            res.release.id == release.id || res.release.date_created > release.date_created
        }
        Some(ResolutionType::InRelease) => {
            if res.release.id == release.id {
                return false;
            }
            if follows_semver {
                if let Some(order) = compare_versions(&res.release.version, &release.version) {
                    return order == Ordering::Greater;
                }
            }
            res.release.date_created >= release.date_created
        }
    }
}

/// Every release of one project, with the queries Sentry runs over them.
// ponytail: holds the project's whole release list in memory; query the
// newest ten and the semver maximum in SQL if a project reaches many thousands.
#[derive(Debug, Clone, Default)]
pub struct ProjectReleases {
    releases: Vec<ReleaseRef>,
    by_version: std::collections::HashMap<String, usize>,
    /// The project half of `follows_semver_versioning_scheme`, which Sentry
    /// caches for an hour: computed once, as the digest asks it per event.
    project_follows_semver: bool,
}

impl ProjectReleases {
    pub fn new(releases: Vec<ReleaseRef>) -> Self {
        let by_version = releases
            .iter()
            .enumerate()
            .map(|(i, r)| (r.version.clone(), i))
            .collect();
        let project_follows_semver = project_follows_semver(&releases);
        Self {
            releases,
            by_version,
            project_follows_semver,
        }
    }

    pub fn releases(&self) -> &[ReleaseRef] {
        &self.releases
    }

    pub fn by_version(&self, version: &str) -> Option<&ReleaseRef> {
        self.by_version.get(version).map(|i| &self.releases[*i])
    }

    /// `follows_semver_versioning_scheme`: among the newest ten releases,
    /// up to two releases need the newest to be semver, under ten need one
    /// semver among the newest three, ten need that and three in all. With
    /// `version`, it must be semver too.
    pub fn follows_semver(&self, version: Option<&str>) -> bool {
        self.project_follows_semver && version.is_none_or(is_semver_version)
    }

    /// `greatest_semver_release`: across packages, by major, minor, patch
    /// and revision, a final above its prereleases, then by build code
    /// (alphanumeric above numeric above none).
    pub fn greatest_semver(&self) -> Option<&ReleaseRef> {
        self.releases
            .iter()
            .filter_map(|r| semver_of(&r.version).map(|v| (semver_key(&v), r)))
            .max_by(|(a, ra), (b, rb)| {
                a.cmp(b)
                    .then_with(|| ra.date_created.cmp(&rb.date_created))
                    .then_with(|| ra.id.cmp(&rb.id))
            })
            .map(|(_, r)| r)
    }

    /// `most_recent_release`: by `COALESCE(date_released, date_added)`.
    pub fn most_recent(&self) -> Option<&ReleaseRef> {
        self.releases
            .iter()
            .max_by(|a, b| a.recency().cmp(&b.recency()).then_with(|| a.id.cmp(&b.id)))
    }

    /// `get_release_to_resolve_by`: the greatest semver release in a semver
    /// project, else the most recent.
    pub fn release_to_resolve_by(&self) -> Option<&ReleaseRef> {
        if self.follows_semver(None) {
            self.greatest_semver()
        } else {
            self.most_recent()
        }
    }

    /// `ReleaseManager.get_next_release`: the first release created after
    /// `current`, equal dates broken by id.
    pub fn next_after(&self, current: &ReleaseRef) -> Option<&ReleaseRef> {
        self.releases
            .iter()
            .filter(|r| {
                r.date_created > current.date_created
                    || (r.date_created == current.date_created && r.id > current.id)
            })
            .min_by(|a, b| a.recency().cmp(&b.recency()).then_with(|| a.id.cmp(&b.id)))
    }
}

/// What a person asked to resolve an issue in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveTarget<'a> {
    /// `status: resolvedInNextRelease`, or `statusDetails.inNextRelease`
    /// (`explicit`), which Sentry's validator anchors on the most recent
    /// release and refuses without one.
    NextRelease { explicit: bool },
    /// `statusDetails.inRelease`: a version, or `latest`.
    Release(&'a str),
}

/// The activity type Sentry records for a release resolution
/// (`ActivityType.SET_RESOLVED_IN_RELEASE`).
pub const ACTIVITY_RESOLVED_IN_RELEASE: &str = "set_resolved_in_release";

/// Sentry's validation message when a project has no release to anchor on.
pub const NO_RELEASE_DATA: &str =
    "No release data present in the system to form a basis for 'Next Release'";
pub const NO_SUCH_RELEASE: &str = "Unable to find a release with the given version.";

/// The `GroupResolution` row a resolution writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionPlan {
    pub kind: ResolutionType,
    pub status: i16,
    pub release: ReleaseRef,
    pub current_release_version: Option<String>,
}

/// What resolving one issue in a release comes to: the row to write, if
/// there is a release to anchor it on, and the activity's data.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolveDecision {
    pub plan: Option<ResolutionPlan>,
    pub activity_data: serde_json::Value,
}

/// The release a target names, as Sentry's `StatusDetailsValidator` and
/// `handle_resolve_in_release` pick it: `Err` is the validation message.
pub fn target_release<'r>(
    releases: &'r ProjectReleases,
    target: ResolveTarget<'_>,
) -> Result<Option<&'r ReleaseRef>, &'static str> {
    match target {
        ResolveTarget::NextRelease { explicit: true } => {
            releases.most_recent().map(Some).ok_or(NO_RELEASE_DATA)
        }
        ResolveTarget::NextRelease { explicit: false } => Ok(releases.release_to_resolve_by()),
        ResolveTarget::Release("latest") => releases.most_recent().map(Some).ok_or(NO_RELEASE_DATA),
        ResolveTarget::Release(version) => releases
            .by_version(version)
            .map(Some)
            .ok_or(NO_SUCH_RELEASE),
    }
}

/// `handle_resolve_in_release` and `process_group_resolution` for one
/// issue whose current release is `last_release` (the release of its newest
/// event).
///
/// In next release: the row remembers the release that is current
/// (`current_release_version`): the greatest semver release in a semver
/// project, else the issue's own current release. Knowing that much already
/// makes it a resolution in a known release: the current one itself under
/// semver (covered up to and including it), or the first release created
/// after it. Only when none exists yet does it stay pending until one does.
pub fn decide(
    releases: &ProjectReleases,
    target: ResolveTarget<'_>,
    last_release: &str,
) -> Result<ResolveDecision, &'static str> {
    let release = target_release(releases, target)?;
    if let ResolveTarget::Release(_) = target {
        let release = release.ok_or(NO_SUCH_RELEASE)?;
        return Ok(ResolveDecision {
            plan: Some(ResolutionPlan {
                kind: ResolutionType::InRelease,
                status: STATUS_RESOLVED,
                release: release.clone(),
                current_release_version: None,
            }),
            activity_data: serde_json::json!({ "version": release.version }),
        });
    }

    // No version yet: the next release is not known.
    let mut activity = serde_json::json!({ "version": "" });
    let Some(release) = release else {
        return Ok(ResolveDecision {
            plan: None,
            activity_data: activity,
        });
    };
    let follows_semver = releases.follows_semver(Some(&release.version));
    let current = if follows_semver {
        releases.greatest_semver()
    } else {
        releases.by_version(last_release)
    };
    let mut plan = ResolutionPlan {
        kind: ResolutionType::InNextRelease,
        status: STATUS_PENDING,
        release: release.clone(),
        current_release_version: current.map(|r| r.version.clone()),
    };
    if let Some(current) = current {
        if follows_semver {
            activity["current_release_version"] = current.version.clone().into();
            plan.release = current.clone();
            plan.kind = ResolutionType::InRelease;
            plan.status = STATUS_RESOLVED;
        } else if let Some(next) = releases.next_after(current) {
            activity["version"] = next.version.clone().into();
            plan.release = next.clone();
            plan.kind = ResolutionType::InRelease;
            plan.status = STATUS_RESOLVED;
        }
    }
    Ok(ResolveDecision {
        plan: Some(plan),
        activity_data: activity,
    })
}

/// `statusDetails` as Sentry's group serializer derives it from the
/// resolution (`GroupSerializerBase._get_status`).
pub fn status_details(kind: Option<ResolutionType>, version: &str) -> String {
    match kind {
        Some(ResolutionType::InRelease) => serde_json::json!({ "inRelease": version }),
        None | Some(ResolutionType::InNextRelease) => serde_json::json!({ "inNextRelease": true }),
    }
    .to_string()
}

/// The project half of [`ProjectReleases::follows_semver`].
fn project_follows_semver(releases: &[ReleaseRef]) -> bool {
    let mut newest: Vec<&ReleaseRef> = releases.iter().collect();
    newest.sort_by(|a, b| {
        b.date_created
            .cmp(&a.date_created)
            .then_with(|| b.id.cmp(&a.id))
    });
    newest.truncate(10);
    let semver: Vec<bool> = newest
        .iter()
        .map(|r| is_semver_version(&r.version))
        .collect();
    let recent = semver.iter().take(3).any(|s| *s);
    match semver.len() {
        0 => false,
        1 | 2 => semver[0],
        3..=9 => recent,
        _ => recent && semver.iter().filter(|s| **s).count() >= 3,
    }
}

/// `SEMVER_COLS_WITH_BUILD_CODE` as an ascending key: `prerelease_case` puts
/// an empty prerelease above any other, `build_code_case` an alphanumeric
/// build code above a numeric one above none.
type SemverKey<'a> = (
    (u64, u64, u64, u64),
    u8,
    &'a str,
    u8,
    Option<u64>,
    Option<&'a str>,
);

fn semver_key<'a>(v: &Version<'a>) -> SemverKey<'a> {
    let pre = v.pre().unwrap_or("");
    let build_number = v.build_number().filter(|n| fits_bigint(*n));
    let build_case = match (build_number, v.build_code()) {
        (None, Some(_)) => 2,
        (Some(_), _) => 1,
        (None, None) => 0,
    };
    (
        v.quad(),
        u8::from(pre.is_empty()),
        pre,
        build_case,
        build_number,
        v.build_code(),
    )
}
