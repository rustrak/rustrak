//! Release ordering and `has_resolution`, ported from Sentry's
//! `tests/sentry/models/test_groupresolution.py`,
//! `tests/sentry/api/helpers/test_group_index.py` (`greatest_semver_release`)
//! and `follows_semver_versioning_scheme` (`src/sentry/models/release.py`).

use chrono::{DateTime, Duration, Utc};
use rustrak::models::resolution::{
    compare_versions, has_resolution, is_semver_version, ProjectReleases, ReleaseRef,
    ResolutionRef, ResolutionType,
};
use std::cmp::Ordering;

fn release(id: i32, version: &str, date_created: DateTime<Utc>) -> ReleaseRef {
    ReleaseRef {
        id,
        version: version.to_string(),
        date_created,
        date_released: None,
    }
}

/// `GroupResolutionTest.setUp`: `a` half an hour old, then `b`, then two
/// semver releases of one package, each created after the one before.
struct Setup {
    old_release: ReleaseRef,
    new_release: ReleaseRef,
    old_semver_release: ReleaseRef,
    new_semver_release: ReleaseRef,
    project: ProjectReleases,
}

fn setup() -> Setup {
    let now = Utc::now();
    let old_release = release(1, "a", now - Duration::minutes(30));
    let new_release = release(2, "b", now);
    let old_semver_release = release(3, "foo_package@1.0", now + Duration::seconds(1));
    let new_semver_release = release(4, "foo_package@2.0", now + Duration::seconds(2));
    let project = ProjectReleases::new(vec![
        old_release.clone(),
        new_release.clone(),
        old_semver_release.clone(),
        new_semver_release.clone(),
    ]);
    Setup {
        old_release,
        new_release,
        old_semver_release,
        new_semver_release,
        project,
    }
}

/// `GroupResolution.has_resolution(group, release)` with the project's own
/// semver decision, as Sentry computes it per event.
fn holds(
    project: &ProjectReleases,
    resolution: Option<&ResolutionRef>,
    event: Option<&ReleaseRef>,
) -> bool {
    let follows = event.is_some_and(|r| project.follows_semver(Some(&r.version)));
    has_resolution(resolution, event, follows)
}

fn resolution<'a>(
    kind: Option<ResolutionType>,
    release: &'a ReleaseRef,
    current: Option<&'a str>,
    project: &'a ProjectReleases,
) -> ResolutionRef<'a> {
    ResolutionRef {
        kind,
        release,
        current_release_version: current,
        current_release: current.and_then(|v| project.by_version(v)),
    }
}

const BOTH: [ResolutionType; 2] = [ResolutionType::InRelease, ResolutionType::InNextRelease];

#[test]
fn in_next_release_with_new_release() {
    let s = setup();
    let r = resolution(
        Some(ResolutionType::InNextRelease),
        &s.old_release,
        None,
        &s.project,
    );
    assert!(!holds(&s.project, Some(&r), Some(&s.new_release)));
}

#[test]
fn in_next_release_with_same_release() {
    let s = setup();
    let r = resolution(
        Some(ResolutionType::InNextRelease),
        &s.old_release,
        None,
        &s.project,
    );
    assert!(holds(&s.project, Some(&r), Some(&s.old_release)));
}

#[test]
fn in_next_release_with_old_release() {
    let s = setup();
    let r = resolution(
        Some(ResolutionType::InNextRelease),
        &s.new_release,
        None,
        &s.project,
    );
    assert!(holds(&s.project, Some(&r), Some(&s.old_release)));
}

#[test]
fn a_resolution_without_a_type_behaves_as_in_next_release() {
    let s = setup();
    let r = resolution(None, &s.new_release, None, &s.project);
    assert!(holds(&s.project, Some(&r), Some(&s.old_release)));
    let r = resolution(None, &s.old_release, None, &s.project);
    assert!(!holds(&s.project, Some(&r), Some(&s.new_release)));
}

#[test]
fn for_semver_when_current_release_version_is_set_with_new_semver_release() {
    let s = setup();
    for kind in BOTH {
        let r = resolution(
            Some(kind),
            &s.old_semver_release,
            Some(&s.old_semver_release.version),
            &s.project,
        );
        assert!(!holds(&s.project, Some(&r), Some(&s.new_semver_release)));
    }
}

#[test]
fn for_semver_when_current_release_version_is_set_with_same_release() {
    let s = setup();
    for kind in BOTH {
        let r = resolution(
            Some(kind),
            &s.old_semver_release,
            Some(&s.old_semver_release.version),
            &s.project,
        );
        assert!(holds(&s.project, Some(&r), Some(&s.old_semver_release)));
    }
}

#[test]
fn for_semver_when_current_release_version_is_set_with_old_semver_release() {
    let s = setup();
    for kind in BOTH {
        let r = resolution(
            Some(kind),
            &s.new_semver_release,
            Some(&s.new_semver_release.version),
            &s.project,
        );
        assert!(holds(&s.project, Some(&r), Some(&s.old_semver_release)));
    }
}

#[test]
fn when_current_release_version_is_set_with_new_release() {
    let s = setup();
    for kind in BOTH {
        let r = resolution(
            Some(kind),
            &s.old_release,
            Some(&s.old_release.version),
            &s.project,
        );
        assert!(!holds(&s.project, Some(&r), Some(&s.new_release)));
    }
}

#[test]
fn when_current_release_version_is_set_with_same_release() {
    let s = setup();
    for kind in BOTH {
        let r = resolution(
            Some(kind),
            &s.old_release,
            Some(&s.old_release.version),
            &s.project,
        );
        assert!(holds(&s.project, Some(&r), Some(&s.old_release)));
    }
}

#[test]
fn when_current_release_version_is_set_with_old_release() {
    let s = setup();
    for kind in BOTH {
        let r = resolution(
            Some(kind),
            &s.new_release,
            Some(&s.new_release.version),
            &s.project,
        );
        assert!(holds(&s.project, Some(&r), Some(&s.old_release)));
    }
}

/// A non-semver `current_release_version` in a semver project: the semver
/// comparison fails and the date model decides.
#[test]
fn when_current_release_version_is_set_incorrect_inputs_fallback_to_older_model() {
    let s = setup();
    let doggo = release(5, "doggo", Utc::now() - Duration::minutes(45));
    let mut all = s.project.releases().to_vec();
    all.push(doggo.clone());
    let project = ProjectReleases::new(all);
    let r = resolution(
        Some(ResolutionType::InNextRelease),
        &doggo,
        Some("doggo"),
        &project,
    );
    for event in [
        &s.old_release,
        &s.new_release,
        &s.old_semver_release,
        &s.new_semver_release,
    ] {
        assert!(!holds(&project, Some(&r), Some(event)), "{}", event.version);
    }
}

/// A `current_release_version` with no release row left: the date model.
#[test]
fn when_current_release_version_is_set_but_does_not_exist_fallback_to_older_model() {
    let s = setup();
    let r = resolution(
        Some(ResolutionType::InNextRelease),
        &s.old_release,
        Some("kittie 12"),
        &s.project,
    );
    for event in [&s.new_release, &s.old_semver_release, &s.new_semver_release] {
        assert!(
            !holds(&s.project, Some(&r), Some(event)),
            "{}",
            event.version
        );
    }
}

#[test]
fn in_release_with_new_release() {
    let s = setup();
    let r = resolution(
        Some(ResolutionType::InRelease),
        &s.old_release,
        None,
        &s.project,
    );
    assert!(!holds(&s.project, Some(&r), Some(&s.new_release)));
}

/// The release it was resolved in is where the fix is: an event from it
/// is a regression.
#[test]
fn in_release_with_current_release() {
    let s = setup();
    let r = resolution(
        Some(ResolutionType::InRelease),
        &s.old_release,
        None,
        &s.project,
    );
    assert!(!holds(&s.project, Some(&r), Some(&s.old_release)));
}

#[test]
fn in_release_with_old_release() {
    let s = setup();
    let r = resolution(
        Some(ResolutionType::InRelease),
        &s.new_release,
        None,
        &s.project,
    );
    assert!(holds(&s.project, Some(&r), Some(&s.old_release)));
}

/// `in_release` compares by date, at or after: a release created at the
/// same instant as the fix (but not the fix itself) is still covered.
#[test]
fn in_release_with_a_release_created_at_the_same_instant() {
    let now = Utc::now();
    let fix = release(1, "fix", now);
    let twin = release(2, "twin", now);
    let project = ProjectReleases::new(vec![fix.clone(), twin.clone()]);
    let r = resolution(Some(ResolutionType::InRelease), &fix, None, &project);
    assert!(holds(&project, Some(&r), Some(&twin)));
}

#[test]
fn for_semver_in_release_with_new_release() {
    let s = setup();
    let r = resolution(
        Some(ResolutionType::InRelease),
        &s.old_semver_release,
        None,
        &s.project,
    );
    assert!(!holds(&s.project, Some(&r), Some(&s.new_semver_release)));
}

#[test]
fn for_semver_in_release_with_current_release() {
    let s = setup();
    let r = resolution(
        Some(ResolutionType::InRelease),
        &s.old_semver_release,
        None,
        &s.project,
    );
    assert!(!holds(&s.project, Some(&r), Some(&s.old_semver_release)));
}

#[test]
fn for_semver_in_release_with_old_release() {
    let s = setup();
    let r = resolution(
        Some(ResolutionType::InRelease),
        &s.new_semver_release,
        None,
        &s.project,
    );
    assert!(holds(&s.project, Some(&r), Some(&s.old_semver_release)));
}

/// Semver wins over dates: an older version created later is still older.
#[test]
fn for_semver_in_release_an_older_version_created_later_is_covered() {
    let now = Utc::now();
    let fixed = release(1, "pkg@2.0.0", now - Duration::hours(1));
    let backport = release(2, "pkg@1.9.0", now);
    let project = ProjectReleases::new(vec![fixed.clone(), backport.clone()]);
    let r = resolution(Some(ResolutionType::InRelease), &fixed, None, &project);
    assert!(holds(&project, Some(&r), Some(&backport)));
}

#[test]
fn no_release_with_resolution() {
    let s = setup();
    let r = resolution(
        Some(ResolutionType::InRelease),
        &s.new_release,
        None,
        &s.project,
    );
    assert!(holds(&s.project, Some(&r), None));
}

#[test]
fn no_release_with_no_resolution() {
    let s = setup();
    assert!(!holds(&s.project, None, None));
    assert!(!holds(&s.project, None, Some(&s.old_release)));
}

/// Resolved in next release when `1.3.0` was current, fixed in `1.5.0`; the
/// project did not follow semver then and does now. `1.4.0` is still older
/// than the fix.
#[test]
fn stale_current_release_version_with_semver_flip() {
    let now = Utc::now();
    let current = release(1, "hotfix_pkg@1.3.0", now - Duration::minutes(45));
    let resolved_in = release(2, "hotfix_pkg@1.5.0", now - Duration::minutes(30));
    let event = release(3, "hotfix_pkg@1.4.0", now - Duration::minutes(15));
    let nightly = release(4, "hotfix_pkg@nightly-100", now - Duration::minutes(60));
    let project = ProjectReleases::new(vec![
        current.clone(),
        resolved_in.clone(),
        event.clone(),
        nightly,
    ]);
    let r = resolution(
        Some(ResolutionType::InNextRelease),
        &resolved_in,
        Some(&current.version),
        &project,
    );
    assert!(holds(&project, Some(&r), Some(&event)));
}

#[test]
fn genuine_regression_detected_with_stale_current_release_version() {
    let now = Utc::now();
    let current = release(1, "regtest_pkg@1.3.0", now - Duration::minutes(45));
    let resolved_in = release(2, "regtest_pkg@1.5.0", now - Duration::minutes(30));
    let event = release(3, "regtest_pkg@1.7.0", now - Duration::minutes(15));
    let project = ProjectReleases::new(vec![current.clone(), resolved_in.clone(), event.clone()]);
    let r = resolution(
        Some(ResolutionType::InNextRelease),
        &resolved_in,
        Some(&current.version),
        &project,
    );
    assert!(!holds(&project, Some(&r), Some(&event)));
}

/// The staggered rollout of GH #576: a fix shipped in `app@1.0.4120` while
/// tenants still run `1.0.4118` and older.
#[test]
fn staggered_rollout_older_builds_do_not_regress() {
    let now = Utc::now();
    let b4117 = release(1, "app@1.0.4117", now - Duration::days(3));
    let b4118 = release(2, "app@1.0.4118", now - Duration::days(2));
    let b4120 = release(3, "app@1.0.4120", now - Duration::days(1));
    let b4121 = release(4, "app@1.0.4121", now);
    let project = ProjectReleases::new(vec![
        b4117.clone(),
        b4118.clone(),
        b4120.clone(),
        b4121.clone(),
    ]);
    // Resolved in next release while 1.0.4118 was the greatest.
    let r = resolution(
        Some(ResolutionType::InRelease),
        &b4118,
        Some(&b4118.version),
        &project,
    );
    assert!(holds(&project, Some(&r), Some(&b4117)));
    assert!(holds(&project, Some(&r), Some(&b4118)));
    assert!(!holds(&project, Some(&r), Some(&b4120)));
    assert!(!holds(&project, Some(&r), Some(&b4121)));
}

// ---------------------------------------------------------------------------
// Version helpers: `Release.is_semver_version` and Relay's `compare_version`.
// ---------------------------------------------------------------------------

#[test]
fn a_semver_version_needs_a_package() {
    assert!(is_semver_version("foo_package@1.0"));
    assert!(is_semver_version("app@1.0.4120"));
    assert!(is_semver_version("app@1.2.3-beta.1+build.7"));
    assert!(!is_semver_version("1.0.4120"));
    assert!(!is_semver_version("a"));
    assert!(!is_semver_version("doggo"));
    assert!(!is_semver_version("hotfix_pkg@nightly-100"));
    assert!(!is_semver_version(""));
    assert!(!is_semver_version("latest"));
    // A build hash is not a version.
    assert!(!is_semver_version(
        "app@a1b2c3d4e5f60718293a4b5c6d7e8f9012345678"
    ));
}

#[test]
fn a_semver_version_fits_a_bigint() {
    assert!(is_semver_version("app@9223372036854775807.0.0"));
    assert!(!is_semver_version("app@9223372036854775808.0.0"));
}

#[test]
fn compare_versions_follows_relay() {
    assert_eq!(compare_versions("app@1.0", "app@2.0"), Some(Ordering::Less));
    assert_eq!(
        compare_versions("app@2.0", "app@2.0.0"),
        Some(Ordering::Equal)
    );
    assert_eq!(
        compare_versions("app@1.0.0", "app@1.0.0-rc1"),
        Some(Ordering::Greater)
    );
    assert_eq!(
        compare_versions("app@1.0.0+20", "app@1.0.0+3"),
        Some(Ordering::Greater)
    );
    // The package is not part of the comparison.
    assert_eq!(compare_versions("a@1.0", "b@2.0"), Some(Ordering::Less));
    assert_eq!(compare_versions("doggo", "app@1.0"), None);
}

// ---------------------------------------------------------------------------
// `follows_semver_versioning_scheme`.
// ---------------------------------------------------------------------------

fn project_of(versions: &[&str]) -> ProjectReleases {
    // Oldest first, a minute apart.
    let start = Utc::now() - Duration::hours(1);
    ProjectReleases::new(
        versions
            .iter()
            .enumerate()
            .map(|(i, v)| release(i as i32 + 1, v, start + Duration::minutes(i as i64)))
            .collect(),
    )
}

#[test]
fn a_project_without_releases_does_not_follow_semver() {
    assert!(!project_of(&[]).follows_semver(None));
}

#[test]
fn with_two_releases_or_fewer_the_newest_decides() {
    assert!(project_of(&["a", "pkg@1.0"]).follows_semver(None));
    assert!(!project_of(&["pkg@1.0", "a"]).follows_semver(None));
    assert!(project_of(&["pkg@1.0"]).follows_semver(None));
}

#[test]
fn under_ten_releases_one_semver_in_the_newest_three_is_enough() {
    assert!(project_of(&["a", "b", "c", "pkg@1.0", "d", "e"]).follows_semver(None));
    assert!(!project_of(&["pkg@1.0", "a", "b", "c"]).follows_semver(None));
}

#[test]
fn with_ten_releases_it_needs_three_semver_and_one_recent() {
    let mut v = vec!["a"; 7];
    v.extend(["pkg@1.0", "pkg@1.1", "pkg@1.2"]);
    assert!(project_of(&v).follows_semver(None));

    // Three semver, none of them in the newest three.
    let mut v = vec!["pkg@1.0", "pkg@1.1", "pkg@1.2"];
    v.extend(["a"; 7]);
    assert!(!project_of(&v).follows_semver(None));

    // A recent one, but only two in ten.
    let mut v = vec!["a"; 8];
    v.extend(["pkg@1.0", "pkg@1.1"]);
    assert!(!project_of(&v).follows_semver(None));

    // Only the newest ten count.
    let mut v = vec!["pkg@0.1", "pkg@0.2", "pkg@0.3"];
    v.extend(["a"; 9]);
    v.push("pkg@1.0");
    assert!(!project_of(&v).follows_semver(None));
}

#[test]
fn the_version_asked_about_must_be_semver_too() {
    let p = project_of(&["pkg@1.0", "pkg@1.1"]);
    assert!(p.follows_semver(Some("pkg@1.2")));
    assert!(!p.follows_semver(Some("a")));
}

// ---------------------------------------------------------------------------
// `greatest_semver_release`, `most_recent_release`, `get_next_release`.
// ---------------------------------------------------------------------------

#[test]
fn greatest_semver_release() {
    let p = project_of(&["foo@2.0.0", "foo@10.0.0", "foo@2.0.0-rc1", "bar"]);
    assert_eq!(p.greatest_semver().unwrap().version, "foo@10.0.0");
}

#[test]
fn greatest_semver_release_prefers_the_final_over_a_prerelease() {
    let p = project_of(&["foo@2.0.0", "foo@2.0.0-rc1"]);
    assert_eq!(p.greatest_semver().unwrap().version, "foo@2.0.0");
}

/// Sentry's `test_greatest_semver_releases_with_build_code`: alphanumeric
/// build codes above numeric ones, numeric ones by number, none lowest.
#[test]
fn greatest_semver_release_with_build_code() {
    let p = project_of(&["foo@1.0.0", "foo@1.0.0+123", "foo@1.0.0+124"]);
    assert_eq!(p.greatest_semver().unwrap().version, "foo@1.0.0+124");
    let p = project_of(&["foo@1.0.0+9", "foo@1.0.0+10"]);
    assert_eq!(p.greatest_semver().unwrap().version, "foo@1.0.0+10");
    let p = project_of(&["foo@1.0.0+124", "foo@1.0.0+abc"]);
    assert_eq!(p.greatest_semver().unwrap().version, "foo@1.0.0+abc");
}

#[test]
fn greatest_semver_release_spans_packages() {
    let p = project_of(&["foo@1.0.0", "bar@3.0.0"]);
    assert_eq!(p.greatest_semver().unwrap().version, "bar@3.0.0");
}

#[test]
fn no_greatest_semver_release_without_semver_releases() {
    assert!(project_of(&["a", "b"]).greatest_semver().is_none());
}

#[test]
fn most_recent_release_prefers_the_finalized_date() {
    let now = Utc::now();
    let mut early = release(1, "early", now - Duration::hours(2));
    early.date_released = Some(now);
    let late = release(2, "late", now - Duration::hours(1));
    let p = ProjectReleases::new(vec![early, late]);
    assert_eq!(p.most_recent().unwrap().version, "early");
}

#[test]
fn next_release_is_the_first_created_after() {
    let p = project_of(&["a", "b", "c"]);
    let a = p.by_version("a").unwrap().clone();
    assert_eq!(p.next_after(&a).unwrap().version, "b");
    let c = p.by_version("c").unwrap().clone();
    assert!(p.next_after(&c).is_none());
}

/// Equal dates fall back to the id, as Sentry's `get_next_release`.
#[test]
fn next_release_with_equal_dates_uses_the_id() {
    let now = Utc::now();
    let p = ProjectReleases::new(vec![
        release(1, "a", now),
        release(2, "b", now),
        release(3, "c", now),
    ]);
    let a = p.by_version("a").unwrap().clone();
    assert_eq!(p.next_after(&a).unwrap().version, "b");
}

#[test]
fn release_to_resolve_by_is_the_greatest_semver_in_a_semver_project() {
    let p = project_of(&["app@2.0.0", "app@1.0.0"]);
    assert_eq!(p.release_to_resolve_by().unwrap().version, "app@2.0.0");
    let p = project_of(&["b", "a"]);
    assert_eq!(p.release_to_resolve_by().unwrap().version, "a");
    assert!(project_of(&[]).release_to_resolve_by().is_none());
}

// ---------------------------------------------------------------------------
// Resolving: `handle_resolve_in_release` / `process_group_resolution`, from
// `tests/sentry/issues/endpoints/test_organization_group_index.py`.
// ---------------------------------------------------------------------------

use rustrak::models::resolution::{
    decide, status_details, target_release, ResolveTarget, NO_RELEASE_DATA, NO_SUCH_RELEASE,
    STATUS_PENDING, STATUS_RESOLVED,
};
use serde_json::json;

const NEXT: ResolveTarget<'static> = ResolveTarget::NextRelease { explicit: false };

/// `test_in_semver_projects_group_resolution_stores_current_release_version`:
/// the greatest semver release is current, whatever the issue last saw, and
/// the resolution is already one in that release.
#[test]
fn in_semver_projects_resolving_in_next_release_stores_the_greatest_release() {
    let p = project_of(&[
        "fake_package@21.1.0",
        "fake_package@21.1.1",
        "fake_package@21.1.2",
    ]);
    let d = decide(&p, NEXT, "fake_package@21.1.0").unwrap();
    let plan = d.plan.unwrap();
    assert_eq!(
        plan.current_release_version.as_deref(),
        Some("fake_package@21.1.2")
    );
    assert_eq!(plan.kind, ResolutionType::InRelease);
    assert_eq!(plan.status, STATUS_RESOLVED);
    assert_eq!(plan.release.version, "fake_package@21.1.2");
    assert_eq!(
        d.activity_data,
        json!({"version": "", "current_release_version": "fake_package@21.1.2"})
    );

    // Releases registered afterwards: anything up to 21.1.2 is covered.
    let now = Utc::now();
    let mut all = p.releases().to_vec();
    all.push(release(
        10,
        "fake_package@21.1.1+1",
        now + Duration::hours(1),
    ));
    all.push(release(11, "fake_package@21.1.3", now + Duration::hours(2)));
    let after = ProjectReleases::new(all);
    let r = ResolutionRef {
        kind: Some(plan.kind),
        release: &plan.release,
        current_release_version: plan.current_release_version.as_deref(),
        current_release: None,
    };
    for v in [
        "fake_package@21.1.0",
        "fake_package@21.1.1",
        "fake_package@21.1.1+1",
        "fake_package@21.1.2",
    ] {
        assert!(holds(&after, Some(&r), after.by_version(v)), "{v}");
    }
    assert!(!holds(
        &after,
        Some(&r),
        after.by_version("fake_package@21.1.3")
    ));
}

/// `test_in_non_semver_projects_group_resolution_stores_current_release_version`:
/// the issue's own last release is current, and a release created after it
/// already exists, so it is resolved in that one.
#[test]
fn in_non_semver_projects_the_issues_last_release_is_current() {
    let now = Utc::now();
    let p = ProjectReleases::new(vec![
        release(1, "foobar 1", now - Duration::minutes(45)),
        release(2, "foobar 2", now),
    ]);
    let d = decide(&p, NEXT, "foobar 1").unwrap();
    let plan = d.plan.unwrap();
    assert_eq!(plan.current_release_version.as_deref(), Some("foobar 1"));
    assert_eq!(plan.kind, ResolutionType::InRelease);
    assert_eq!(plan.release.version, "foobar 2");
    assert_eq!(d.activity_data, json!({"version": "foobar 2"}));

    // `foobar 3`, registered between the two, regresses as `foobar 2` does.
    let mut all = p.releases().to_vec();
    all.push(release(3, "foobar 3", now - Duration::minutes(30)));
    let after = ProjectReleases::new(all);
    let r = ResolutionRef {
        kind: Some(plan.kind),
        release: &plan.release,
        current_release_version: plan.current_release_version.as_deref(),
        current_release: after.by_version("foobar 1"),
    };
    assert!(holds(&after, Some(&r), after.by_version("foobar 1")));
    assert!(!holds(&after, Some(&r), after.by_version("foobar 2")));
    assert!(!holds(&after, Some(&r), after.by_version("foobar 3")));
}

/// No release after the current one yet: pending, anchored on the release
/// to resolve by, until `clear_expired_resolutions` names the next one.
#[test]
fn in_non_semver_projects_without_a_later_release_it_stays_pending() {
    let p = project_of(&["a", "b"]);
    let d = decide(&p, NEXT, "b").unwrap();
    let plan = d.plan.unwrap();
    assert_eq!(plan.kind, ResolutionType::InNextRelease);
    assert_eq!(plan.status, STATUS_PENDING);
    assert_eq!(plan.release.version, "b");
    assert_eq!(plan.current_release_version.as_deref(), Some("b"));
    assert_eq!(d.activity_data, json!({"version": ""}));
}

/// An issue whose last release has no row (or no release at all) records
/// no current version: the older model decides on the anchor's date.
#[test]
fn an_issue_without_a_known_last_release_has_no_current_version() {
    let p = project_of(&["a", "b"]);
    let plan = decide(&p, NEXT, "").unwrap().plan.unwrap();
    assert_eq!(plan.kind, ResolutionType::InNextRelease);
    assert_eq!(plan.current_release_version, None);
    let plan = decide(&p, NEXT, "never-registered").unwrap().plan.unwrap();
    assert_eq!(plan.current_release_version, None);
}

/// `status: resolvedInNextRelease` in a project with no release: resolved,
/// with nothing to anchor on.
#[test]
fn resolving_in_next_release_without_releases_writes_no_resolution() {
    let d = decide(&project_of(&[]), NEXT, "").unwrap();
    assert_eq!(d.plan, None);
    assert_eq!(d.activity_data, json!({"version": ""}));
}

/// `statusDetails.inNextRelease` needs a release (`validate_inNextRelease`).
#[test]
fn explicit_in_next_release_without_releases_is_refused() {
    let target = ResolveTarget::NextRelease { explicit: true };
    assert_eq!(decide(&project_of(&[]), target, ""), Err(NO_RELEASE_DATA));
}

/// `statusDetails.inNextRelease` anchors on the most recent release, not the
/// greatest semver one.
#[test]
fn explicit_in_next_release_anchors_on_the_most_recent_release() {
    let p = project_of(&["app@2.0.0", "app@1.0.0"]);
    let target = ResolveTarget::NextRelease { explicit: true };
    // The anchor: the most recent release, where the status alone would
    // take the greatest semver one.
    assert_eq!(
        target_release(&p, target)
            .unwrap()
            .map(|r| r.version.as_str()),
        Some("app@1.0.0")
    );
    assert_eq!(
        target_release(&p, NEXT)
            .unwrap()
            .map(|r| r.version.as_str()),
        Some("app@2.0.0")
    );
    let plan = decide(&p, target, "app@1.0.0").unwrap().plan.unwrap();
    // Semver still names the current release, and resolves in it.
    assert_eq!(plan.current_release_version.as_deref(), Some("app@2.0.0"));
    assert_eq!(plan.release.version, "app@2.0.0");
}

/// `test_set_resolved_in_explicit_release`.
#[test]
fn resolving_in_an_explicit_release() {
    let p = project_of(&["a", "b"]);
    let d = decide(&p, ResolveTarget::Release("a"), "b").unwrap();
    let plan = d.plan.unwrap();
    assert_eq!(plan.kind, ResolutionType::InRelease);
    assert_eq!(plan.status, STATUS_RESOLVED);
    assert_eq!(plan.release.version, "a");
    assert_eq!(plan.current_release_version, None);
    assert_eq!(d.activity_data, json!({"version": "a"}));
}

/// `test_in_semver_projects_set_resolved_in_explicit_release`: no current
/// version, even under semver.
#[test]
fn resolving_in_an_explicit_semver_release() {
    let p = project_of(&["app@1.0.0", "app@2.0.0", "app@3.0.0"]);
    let plan = decide(&p, ResolveTarget::Release("app@2.0.0"), "app@1.0.0")
        .unwrap()
        .plan
        .unwrap();
    assert_eq!(plan.release.version, "app@2.0.0");
    assert_eq!(plan.current_release_version, None);
}

/// `test_set_resolved_in_current_release`: `latest` is the most recent.
#[test]
fn resolving_in_the_latest_release() {
    let p = project_of(&["a", "b"]);
    let plan = decide(&p, ResolveTarget::Release("latest"), "a")
        .unwrap()
        .plan
        .unwrap();
    assert_eq!(plan.release.version, "b");
    assert_eq!(
        decide(&project_of(&[]), ResolveTarget::Release("latest"), ""),
        Err(NO_RELEASE_DATA)
    );
}

#[test]
fn resolving_in_an_unknown_release_is_refused() {
    let p = project_of(&["a"]);
    assert_eq!(
        decide(&p, ResolveTarget::Release("nope"), "a"),
        Err(NO_SUCH_RELEASE)
    );
}

#[test]
fn status_details_follow_the_resolution_type() {
    assert_eq!(
        status_details(Some(ResolutionType::InRelease), "app@1.0"),
        r#"{"inRelease":"app@1.0"}"#
    );
    assert_eq!(
        status_details(Some(ResolutionType::InNextRelease), "x"),
        r#"{"inNextRelease":true}"#
    );
    assert_eq!(status_details(None, "x"), r#"{"inNextRelease":true}"#);
}
