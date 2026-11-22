//! The Web Vitals performance score, ported from Relay's
//! `normalize_performance_score` with the profiles Sentry configures.
//! Expected ratios are Relay's own fixtures
//! (relay-event-normalization/src/event.rs, b95381eca).

use rustrak::services::web_vitals::{browser_name, device, score, total_score, Vital};

fn ratio(scored: &[(Vital, f64)], vital: Vital) -> Option<f64> {
    scored.iter().find(|(v, _)| *v == vital).map(|(_, r)| *r)
}

fn close(actual: Option<f64>, expected: f64) {
    let actual = actual.expect("vital was scored");
    assert!(
        (actual - expected).abs() < 1e-9,
        "{actual} is not {expected}"
    );
}

#[test]
fn chrome_pageload_scores_each_vital_like_relay() {
    let scored = score(
        &[
            (Vital::Fcp, 1237.0),
            (Vital::Lcp, 6596.0),
            (Vital::Cls, 0.11),
        ],
        Some("Chrome"),
    );

    close(ratio(&scored, Vital::Fcp), 0.7167236962527221);
    close(ratio(&scored, Vital::Lcp), 0.03079632190462195);
    close(ratio(&scored, Vital::Cls), 0.8745668242977945);
}

#[test]
fn a_mobile_browser_is_scored_on_mobile_thresholds() {
    // 4000ms is the mobile LCP median, far past the desktop one.
    let lcp = |browser| ratio(&score(&[(Vital::Lcp, 4000.0)], Some(browser)), Vital::Lcp);

    close(lcp("Chrome Mobile"), 0.5);
    assert!(lcp("Chrome").unwrap() < 0.2);
}

#[test]
fn mobile_safari_without_a_weighted_vital_falls_back_to_the_default_profile() {
    // Safari Mobile weighs LCP at 0, so LCP alone gives it nothing to weigh
    // and Relay scores the event with "Default", on desktop thresholds.
    let alone = score(&[(Vital::Lcp, 4000.0)], Some("Mobile Safari"));
    let with_fcp = score(
        &[(Vital::Lcp, 4000.0), (Vital::Fcp, 3000.0)],
        Some("Mobile Safari"),
    );

    let desktop = score(&[(Vital::Lcp, 4000.0)], Some("Chrome"));
    close(
        ratio(&alone, Vital::Lcp),
        ratio(&desktop, Vital::Lcp).unwrap(),
    );
    close(ratio(&with_fcp, Vital::Lcp), 0.5);
    close(ratio(&with_fcp, Vital::Fcp), 0.5);
}

#[test]
fn an_interaction_is_scored_on_its_own_inp_profile() {
    // Pageload profiles have no INP: an interaction falls through to the
    // INP ones, where INP weighs everything and its median is 500ms.
    close(
        ratio(&score(&[(Vital::Inp, 500.0)], Some("Chrome")), Vital::Inp),
        0.5,
    );
    close(
        ratio(&score(&[(Vital::Inp, 500.0)], Some("Firefox")), Vital::Inp),
        0.5,
    );
}

#[test]
fn nothing_is_scored_without_a_vital() {
    assert!(score(&[], Some("Chrome")).is_empty());
}

#[test]
fn the_browser_is_named_as_relay_names_it() {
    // ua-parser's family names, which Sentry's profile conditions compare.
    let cases = [
        ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36", Some("Chrome")),
        ("Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36", Some("Chrome Mobile")),
        ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Safari/605.1.15", Some("Safari")),
        ("Mozilla/5.0 (iPhone; CPU iPhone OS 18_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Mobile/15E148 Safari/604.1", Some("Mobile Safari")),
        ("Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:133.0) Gecko/20100101 Firefox/133.0", Some("Firefox")),
        ("Mozilla/5.0 (Android 14; Mobile; rv:133.0) Gecko/133.0 Firefox/133.0", Some("Firefox Mobile")),
        ("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0", Some("Edge")),
        ("Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36 EdgA/131.0.0.0", Some("Edge Mobile")),
        ("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 OPR/115.0.0.0", Some("Opera")),
        ("Mozilla/5.0 (Linux; Android 10; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Mobile Safari/537.36 OPR/86.0.0.0", Some("Opera Mobile")),
        ("Mozilla/5.0 (iPhone; CPU iPhone OS 18_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/131.0.6778.73 Mobile/15E148 Safari/604.1", Some("Chrome Mobile iOS")),
        ("curl/8.7.1", None),
    ];
    for (user_agent, expected) in cases {
        assert_eq!(browser_name(user_agent), expected, "{user_agent}");
    }
}

#[test]
fn the_total_weighs_only_the_vitals_with_a_score() {
    // LCP 0.30 and FCP 0.15 out of the 0.45 they have between them.
    close(
        total_score([(Vital::Lcp, 1.0), (Vital::Fcp, 0.0)]),
        0.30 / 0.45,
    );
    assert_eq!(total_score([]), None);
}

#[test]
fn a_mobile_browser_is_a_phone_and_any_other_a_desktop() {
    assert_eq!(device(Some("Chrome Mobile")), Some("mobile"));
    assert_eq!(device(Some("Mobile Safari")), Some("mobile"));
    assert_eq!(device(Some("Chrome Mobile iOS")), Some("mobile"));
    assert_eq!(device(Some("Chrome")), Some("desktop"));
    assert_eq!(device(Some("Firefox")), Some("desktop"));
    assert_eq!(device(None), None);
}

#[test]
fn each_vital_is_rated_on_web_dev_thresholds() {
    assert_eq!(Vital::Lcp.rating(2500.0), "good");
    assert_eq!(Vital::Lcp.rating(2501.0), "meh");
    assert_eq!(Vital::Lcp.rating(4001.0), "poor");
    assert_eq!(Vital::Cls.rating(0.1), "good");
    assert_eq!(Vital::Inp.rating(500.0), "meh");
}
