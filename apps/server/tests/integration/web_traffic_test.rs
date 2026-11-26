//! Web Traffic, the first real plugin, end to end: built by its author's
//! CLI, installed, fed by its tracker's beacons, and read by its report.

use actix_web::http::StatusCode;
use actix_web::test;
use rustrak::plugins::host::Source;
use serde_json::{json, Value};

use rustrak::models::UserRole;

use crate::common::{self, plugins::web_traffic_rtp, TestDb};

const FIREFOX: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 14.5; rv:127.0) Gecko/20100101 Firefox/127.0";
const IPHONE: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_5 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.5 Mobile/15E148 Safari/604.1";
const GOOGLEBOT: &str = "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)";

fn bearer(token: &str) -> (&'static str, String) {
    ("Authorization", format!("Bearer {token}"))
}

macro_rules! call {
    ($app:expr, $req:expr) => {{
        let resp = test::call_service($app, $req.to_request()).await;
        let status = resp.status();
        let body = test::read_body(resp).await;
        (
            status,
            serde_json::from_slice::<Value>(&body).unwrap_or(Value::Null),
        )
    }};
}

/// A beacon as `script.js` sends it: text/plain JSON, from a visitor's
/// address and browser.
fn beacon(project: i32, key: &str, ip: &str, ua: &str, body: Value) -> test::TestRequest {
    test::TestRequest::post()
        .uri(&format!(
            "/api/projects/{project}/plugins/web-traffic/collect?sentry_key={key}"
        ))
        .peer_addr(format!("{ip}:5000").parse().unwrap())
        .insert_header(("User-Agent", ua))
        .insert_header(("Content-Type", "text/plain;charset=UTF-8"))
        .insert_header(("Origin", "https://shop.example.com"))
        .set_payload(body.to_string())
}

/// A ranking's rows as value, visitors and count: what most assertions read.
fn ranked(rows: &Value) -> Value {
    Value::Array(
        rows.as_array()
            .unwrap()
            .iter()
            .map(|r| json!({ "value": r["value"], "visitors": r["visitors"], "count": r["count"] }))
            .collect(),
    )
}

fn pageview(url: &str) -> Value {
    json!({ "type": "pageview", "url": url, "language": "es-ES" })
}

/// Installed and on in every project, with one project to count.
async fn installed() -> (
    TestDb,
    impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    >,
    String,
    i32,
    String,
) {
    let db = TestDb::new().await;
    let state = common::state(&db.pool, common::config()).await;
    state
        .plugins
        .install(&db.pool, web_traffic_rtp(), Source::Upload)
        .await
        .unwrap();
    let app = common::app_with(&state).await;
    let owner = common::create_test_token(&db.pool).await;
    let (status, _) = call!(
        &app,
        test::TestRequest::put()
            .uri("/api/settings/plugins/web-traffic")
            .insert_header(bearer(&owner))
            .set_json(json!({ "allProjects": true }))
    );
    assert_eq!(status, StatusCode::OK);
    let (project, key) = common::create_ingest_project(&db.pool, "shop").await;
    (db, app, owner, project, key)
}

#[actix_web::test]
async fn a_websites_visits_become_its_traffic_report() {
    let (_db, app, owner, project, key) = installed().await;

    // Ana reads two pages, coming from Hacker News on a campaign link.
    let (status, _) = call!(
        &app,
        beacon(
            project,
            &key,
            "10.0.0.1",
            FIREFOX,
            json!({
                "type": "pageview",
                "url": "https://shop.example.com/?utm_source=hn&utm_campaign=launch",
                "referrer": "https://news.ycombinator.com/item?id=1",
                "title": "Shop",
                "language": "es-ES",
            })
        )
    );
    assert_eq!(status, StatusCode::NO_CONTENT);
    call!(
        &app,
        beacon(
            project,
            &key,
            "10.0.0.1",
            FIREFOX,
            pageview("https://shop.example.com/pricing")
        )
    );
    // Ben reads one page on his phone, behind Cloudflare in Germany.
    call!(
        &app,
        beacon(
            project,
            &key,
            "10.0.0.2",
            IPHONE,
            pageview("https://shop.example.com/pricing")
        )
        .insert_header(("CF-IPCountry", "DE"))
    );
    // A crawler is not a visitor.
    call!(
        &app,
        beacon(
            project,
            &key,
            "10.0.0.3",
            GOOGLEBOT,
            pageview("https://shop.example.com/")
        )
    );

    let get = |path: &str| {
        test::TestRequest::get()
            .uri(&format!(
                "/api/projects/{project}/plugins/web-traffic{path}"
            ))
            .insert_header(bearer(&owner))
    };
    let (status, overview) = call!(&app, get("/overview?period=24h"));
    assert_eq!(status, StatusCode::OK, "{overview}");
    assert_eq!(
        overview["current"],
        json!({ "pageviews": 3, "visitors": 2, "visits": 2, "bounces": 1, "duration": overview["current"]["duration"] })
    );
    assert_eq!(overview["previous"]["pageviews"], 0);
    let series = overview["series"].as_array().unwrap();
    assert_eq!(series.len(), 24, "every hour, the empty ones too");
    assert_eq!(
        series.last().unwrap()["pageviews"],
        3,
        "now is the last hour"
    );

    let top = |by: &str| get(&format!("/top?by={by}&period=24h"));
    let (_, pages) = call!(&app, top("path"));
    assert_eq!(
        ranked(&pages),
        json!([
            { "value": "/pricing", "visitors": 2, "count": 2 },
            { "value": "/", "visitors": 1, "count": 1 },
        ])
    );
    let (_, referrers) = call!(&app, top("referrer"));
    assert_eq!(
        ranked(&referrers),
        json!([{ "value": "news.ycombinator.com", "visitors": 1, "count": 1 }])
    );
    let (_, sources) = call!(&app, top("source"));
    assert_eq!(sources[0]["value"], "hn");
    let (_, campaigns) = call!(&app, top("campaign"));
    assert_eq!(campaigns[0]["value"], "launch");
    let (_, countries) = call!(&app, top("country"));
    let countries: Vec<&str> = countries
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["value"].as_str().unwrap())
        .collect();
    assert_eq!(
        countries,
        ["ES", "DE"],
        "the proxy's country, else the language's"
    );
    let (_, devices) = call!(&app, top("device"));
    assert_eq!(devices.as_array().unwrap().len(), 2);
    let (status, _) = call!(&app, top("visitor"));
    assert_eq!(status, StatusCode::BAD_REQUEST, "only a known dimension");

    let (_, page) = call!(&app, get("/page?path=/pricing"));
    assert_eq!(page, json!({ "pageviews": 2, "visitors": 2 }));
    let (_, live) = call!(&app, get("/live"));
    assert_eq!(live, json!({ "visitors": 2 }));
}

/// Narrowed to a country, a browser or a page, every figure, list and chart
/// agrees: they are one report seen through one filter.
#[actix_web::test]
async fn one_filter_narrows_the_whole_report() {
    let (_db, app, owner, project, key) = installed().await;
    // Ana (Spain, Firefox): home, then pricing, then signs up.
    for url in [
        "https://shop.example.com/",
        "https://shop.example.com/pricing",
    ] {
        call!(
            &app,
            beacon(project, &key, "10.0.0.1", FIREFOX, pageview(url))
                .insert_header(("CF-IPCountry", "ES"))
        );
    }
    call!(
        &app,
        beacon(
            project,
            &key,
            "10.0.0.1",
            FIREFOX,
            json!({ "type": "event", "url": "https://shop.example.com/pricing", "name": "signup", "data": { "plan": "pro", "seats": 3 } })
        )
    );
    // Ben (Germany, iPhone): lands on pricing and leaves.
    call!(
        &app,
        beacon(
            project,
            &key,
            "10.0.0.2",
            IPHONE,
            pageview("https://shop.example.com/pricing")
        )
        .insert_header(("CF-IPCountry", "DE"))
    );
    call!(
        &app,
        beacon(
            project,
            &key,
            "10.0.0.2",
            IPHONE,
            json!({ "type": "event", "url": "https://shop.example.com/pricing", "name": "signup", "data": { "plan": "free" } })
        )
    );

    let get = |path: &str| {
        test::TestRequest::get()
            .uri(&format!(
                "/api/projects/{project}/plugins/web-traffic{path}"
            ))
            .insert_header(bearer(&owner))
    };
    let (_, spain) = call!(&app, get("/overview?country=ES"));
    assert_eq!(spain["current"]["visitors"], 1);
    assert_eq!(spain["current"]["pageviews"], 2);
    assert_eq!(spain["current"]["bounces"], 0);
    let (_, pricing) = call!(&app, get("/overview?path=/pricing"));
    assert_eq!(
        pricing["current"]["visitors"], 2,
        "the page is `path`: `page` is a list's"
    );
    let (_, pages) = call!(&app, get("/top?by=path&browser=Safari"));
    assert_eq!(
        ranked(&pages),
        json!([{ "value": "/pricing", "visitors": 1, "count": 1 }])
    );
    let (_, events) = call!(&app, get("/top?by=event&country=DE"));
    assert_eq!(
        ranked(&events),
        json!([{ "value": "signup", "visitors": 1, "count": 1 }]),
        "a visitor's events follow the filter"
    );

    // Where visits start and end.
    let (_, entry) = call!(&app, get("/top?by=entry"));
    assert_eq!(
        ranked(&entry),
        json!([
            { "value": "/", "visitors": 1, "count": 1 },
            { "value": "/pricing", "visitors": 1, "count": 1 },
        ])
    );
    let (_, exit) = call!(&app, get("/top?by=exit"));
    assert_eq!(
        ranked(&exit),
        json!([{ "value": "/pricing", "visitors": 2, "count": 2 }])
    );
    // A full list, searched.
    let (status, found) = call!(&app, get("/top?by=path&q=pric&limit=200"));
    assert_eq!(status, StatusCode::OK, "{found}");
    assert_eq!(found.as_array().unwrap().len(), 1);

    // Who visited, and what each one did.
    let (_, visitors) = call!(&app, get("/visitors"));
    let visitors = visitors.as_array().unwrap();
    assert_eq!(visitors.len(), 2);
    let ana = visitors.iter().find(|v| v["country"] == "ES").unwrap();
    assert_eq!(ana["pageviews"], 2);
    assert_eq!(ana["events"], 1);
    assert_eq!(ana["entry"], "/");
    assert_eq!(ana["browser"], "Firefox");
    let (_, journey) = call!(
        &app,
        get(&format!("/journey?id={}", ana["id"].as_str().unwrap()))
    );
    let steps: Vec<(&str, &str)> = journey
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["kind"].as_str().unwrap(), s["path"].as_str().unwrap()))
        .collect();
    assert_eq!(
        steps,
        [
            ("pageview", "/"),
            ("pageview", "/pricing"),
            ("event", "/pricing")
        ]
    );
    let (status, _) = call!(&app, get("/journey?id=nobody"));
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Each value with how its visitors' days went.
    let (_, full) = call!(&app, get("/top?by=country"));
    let spain = full
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["value"] == "ES")
        .unwrap();
    assert_eq!(
        (spain["bounces"].as_i64(), spain["visitors"].as_i64()),
        (Some(0), Some(1))
    );
    let germany = full
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["value"] == "DE")
        .unwrap();
    assert_eq!(germany["bounces"], 1, "one page and gone");

    // A visitor's profile.
    let (_, profile) = call!(
        &app,
        get(&format!("/visitor?id={}", ana["id"].as_str().unwrap()))
    );
    assert_eq!(profile["entry"], "/");
    assert_eq!(profile["exit"], "/pricing");
    assert_eq!(profile["pageviews"], 2);
    assert_eq!(profile["pages"], 2);
    assert_eq!(profile["events"], 1);
    assert_eq!(profile["country"], "ES");
    let (status, _) = call!(&app, get("/visitor?id=nobody"));
    assert_eq!(status, StatusCode::NOT_FOUND);

    // An event over time, where it happened, and its latest occurrences.
    // Over time: every hour of the day, the empty ones too.
    let (_, series) = call!(&app, get("/event-series?name=signup&period=24h"));
    assert_eq!(series["names"], json!(["signup"]));
    let rows = series["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 24);
    let total: i64 = rows.iter().map(|r| r["0"].as_i64().unwrap()).sum();
    assert_eq!(total, 2);
    let (_, all) = call!(&app, get("/event-series?period=24h"));
    assert_eq!(all["names"], json!(["signup"]), "the most common events");
    // Exactly one event, whatever others share its name's letters.
    let (_, totals) = call!(&app, get("/event?name=signup"));
    assert_eq!(totals, json!({ "count": 2, "visitors": 2, "everyone": 2 }));
    let (_, pages) = call!(&app, get("/event-pages?name=signup"));
    assert_eq!(
        ranked(&pages),
        json!([{ "value": "/pricing", "visitors": 2, "count": 2 }])
    );
    let (_, recent) = call!(&app, get("/event-recent?name=signup&country=ES"));
    assert_eq!(recent.as_array().unwrap().len(), 1, "{recent}");
    assert_eq!(recent[0]["visitor"], ana["id"]);

    // What an event carried.
    let (_, properties) = call!(&app, get("/event-properties?name=signup"));
    assert_eq!(
        properties,
        json!([
            { "key": "plan", "value": "free", "count": 1 },
            { "key": "plan", "value": "pro", "count": 1 },
            { "key": "seats", "value": "3", "count": 1 },
        ])
    );
}

#[actix_web::test]
async fn a_websites_own_events_are_counted() {
    let (_db, app, owner, project, key) = installed().await;
    for ip in ["10.0.0.1", "10.0.0.2", "10.0.0.2"] {
        let (status, _) = call!(
            &app,
            beacon(
                project,
                &key,
                ip,
                FIREFOX,
                json!({ "type": "event", "url": "https://shop.example.com/pricing", "name": "signup", "data": { "plan": "pro" } })
            )
        );
        assert_eq!(status, StatusCode::NO_CONTENT);
    }
    let (status, _) = call!(
        &app,
        beacon(
            project,
            &key,
            "10.0.0.1",
            FIREFOX,
            json!({ "type": "event", "url": "https://shop.example.com/" })
        )
    );
    assert_eq!(status, StatusCode::BAD_REQUEST, "an event needs a name");

    let (_, events) = call!(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/projects/{project}/plugins/web-traffic/top?by=event"
            ))
            .insert_header(bearer(&owner))
    );
    assert_eq!(
        ranked(&events),
        json!([{ "value": "signup", "visitors": 2, "count": 3 }])
    );
}

/// Whatever a website names its events or puts in them, its report reads it,
/// and a conversion is out of every visitor, the ones who only sent events too.
#[actix_web::test]
async fn any_event_name_and_data_are_reported() {
    let (_db, app, owner, project, key) = installed().await;
    // Two visitors only send events (the tracker's auto-track off), named
    // like the chart's own time key and carrying a null.
    for ip in ["10.0.0.1", "10.0.0.2"] {
        let (status, _) = call!(
            &app,
            beacon(
                project,
                &key,
                ip,
                FIREFOX,
                json!({ "type": "event", "url": "https://shop.example.com/", "name": "t", "data": { "plan": null } })
            )
        );
        assert_eq!(status, StatusCode::NO_CONTENT);
    }
    // A third only reads a page.
    call!(
        &app,
        beacon(
            project,
            &key,
            "10.0.0.3",
            FIREFOX,
            pageview("https://shop.example.com/")
        )
    );
    let get = |path: &str| {
        test::TestRequest::get()
            .uri(&format!(
                "/api/projects/{project}/plugins/web-traffic{path}"
            ))
            .insert_header(bearer(&owner))
    };

    let (status, properties) = call!(&app, get("/event-properties?name=t"));
    assert_eq!(status, StatusCode::OK, "{properties}");
    assert_eq!(
        properties,
        json!([{ "key": "plan", "value": "null", "count": 2 }])
    );

    let (_, series) = call!(&app, get("/event-series?name=t&period=24h"));
    assert_eq!(series["names"], json!(["t"]));
    let rows = series["rows"].as_array().unwrap();
    assert!(
        rows.windows(2)
            .all(|w| w[1]["t"].as_i64().unwrap() - w[0]["t"].as_i64().unwrap() == 3_600),
        "every row keeps its hour: {series}"
    );
    let total: i64 = rows.iter().map(|r| r["0"].as_i64().unwrap()).sum();
    assert_eq!(
        total, 2,
        "an event's counts are keyed by its place in names"
    );

    let (_, totals) = call!(&app, get("/event?name=t"));
    assert_eq!(
        totals,
        json!({ "count": 2, "visitors": 2, "everyone": 3 }),
        "two of three visitors, not two of the one who read a page"
    );

    // Who did it links to their day, page views or not.
    let (_, recent) = call!(&app, get("/event-recent?name=t"));
    let who = recent[0]["visitor"].as_str().unwrap().to_string();
    let (status, profile) = call!(&app, get(&format!("/visitor?id={who}")));
    assert_eq!(status, StatusCode::OK, "{profile}");
    assert_eq!(
        (&profile["pageviews"], &profile["events"], &profile["entry"]),
        (&json!(0), &json!(1), &json!("/"))
    );
}

/// A visit ends at a half-hour pause, as the visitor page splits a day: the
/// time away is no visit's length.
#[actix_web::test]
async fn a_visit_ends_at_a_long_pause() {
    let db = TestDb::new().await;
    let config = common::config();
    let data = config.plugins_dir.join("web-traffic").join("data.sqlite");
    let state = common::state(&db.pool, config).await;
    state
        .plugins
        .install(&db.pool, web_traffic_rtp(), Source::Upload)
        .await
        .unwrap();
    let app = common::app_with(&state).await;
    let owner = common::create_test_token(&db.pool).await;
    call!(
        &app,
        test::TestRequest::put()
            .uri("/api/settings/plugins/web-traffic")
            .insert_header(bearer(&owner))
            .set_json(json!({ "allProjects": true }))
    );
    let (project, key) = common::create_ingest_project(&db.pool, "shop").await;
    for _ in 0..4 {
        call!(
            &app,
            beacon(
                project,
                &key,
                "10.0.0.1",
                FIREFOX,
                pageview("https://shop.example.com/")
            )
        );
    }
    // Two pages a minute apart eight hours ago, and two a minute apart now.
    rusqlite::Connection::open(&data)
        .unwrap()
        .execute(
            "UPDATE pageviews SET created_at = unixepoch() - CASE id \
             WHEN 1 THEN 28860 WHEN 2 THEN 28800 WHEN 3 THEN 60 ELSE 0 END",
            [],
        )
        .unwrap();

    let get = |path: &str| {
        test::TestRequest::get()
            .uri(&format!(
                "/api/projects/{project}/plugins/web-traffic{path}"
            ))
            .insert_header(bearer(&owner))
    };
    let (_, overview) = call!(&app, get("/overview?period=24h"));
    assert_eq!(
        (
            &overview["current"]["duration"],
            &overview["current"]["visits"]
        ),
        (&json!(120), &json!(2)),
        "{overview}"
    );
    let (_, pages) = call!(&app, get("/top?by=path&period=24h"));
    assert_eq!(
        (&pages[0]["duration"], &pages[0]["visits"]),
        (&json!(120), &json!(2)),
        "{pages}"
    );
}

#[actix_web::test]
async fn what_should_not_count_is_dropped_quietly() {
    let (_db, app, owner, project, key) = installed().await;

    // Only the shop's own domains, set for this project.
    let (status, body) = call!(
        &app,
        test::TestRequest::put()
            .uri(&format!(
                "/api/projects/{project}/plugin-settings/web-traffic"
            ))
            .insert_header(bearer(&owner))
            .set_json(json!({ "allowedDomains": "example.com" }))
    );
    assert_eq!(status, StatusCode::OK, "{body}");

    let counted = [
        beacon(
            project,
            &key,
            "10.0.0.1",
            FIREFOX,
            pageview("https://shop.example.com/"),
        ),
        beacon(
            project,
            &key,
            "10.0.0.2",
            FIREFOX,
            pageview("https://example.com/"),
        ),
    ];
    let dropped = [
        // Someone else's site carrying the shop's key.
        beacon(
            project,
            &key,
            "10.0.0.3",
            FIREFOX,
            pageview("https://copycat.net/"),
        ),
        // A visitor asking not to be tracked (respected by default).
        beacon(
            project,
            &key,
            "10.0.0.4",
            FIREFOX,
            pageview("https://example.com/"),
        )
        .insert_header(("DNT", "1")),
        beacon(
            project,
            &key,
            "10.0.0.5",
            "curl/8.6.0",
            pageview("https://example.com/"),
        ),
    ];
    for req in counted.into_iter().chain(dropped) {
        let (status, _) = call!(&app, req);
        assert_eq!(status, StatusCode::NO_CONTENT);
    }
    let (status, _) = call!(
        &app,
        beacon(
            project,
            &key,
            "10.0.0.1",
            FIREFOX,
            pageview("javascript:alert(1)")
        )
    );
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call!(
        &app,
        beacon(project, &key, "10.0.0.1", FIREFOX, json!("nonsense"))
    );
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = call!(
        &app,
        beacon(
            project,
            "0123456789abcdef0123456789abcdef",
            "10.0.0.1",
            FIREFOX,
            pageview("https://example.com/")
        )
    );
    assert_eq!(status, StatusCode::UNAUTHORIZED, "another project's key");

    let (_, overview) = call!(
        &app,
        test::TestRequest::get()
            .uri(&format!(
                "/api/projects/{project}/plugins/web-traffic/overview"
            ))
            .insert_header(bearer(&owner))
    );
    assert_eq!(overview["current"]["pageviews"], 2, "{overview}");
}

#[actix_web::test]
async fn the_report_needs_its_permission() {
    let (db, app, _owner, project, key) = installed().await;
    call!(
        &app,
        beacon(
            project,
            &key,
            "10.0.0.1",
            FIREFOX,
            pageview("https://shop.example.com/")
        )
    );

    // Three people on the shop: one who reads issues, one who reads the
    // traffic, and one who may also look at visitors one by one.
    let traffic = json!({ "resource": "web-traffic:traffic", "action": "read" });
    let visitors = json!({ "resource": "web-traffic:visitors", "action": "read" });
    let mut tokens = vec![];
    for (email, plugin_permissions) in [
        ("issues@example.com", json!([])),
        ("traffic@example.com", json!([traffic])),
        ("visitors@example.com", json!([traffic, visitors])),
    ] {
        let (status, role) = call!(
            &app,
            test::TestRequest::post()
                .uri("/api/roles")
                .insert_header(bearer(&_owner))
                .set_json(json!({
                    "name": email,
                    "permissions": [{ "resource": "issues", "action": "read" }],
                    "plugin_permissions": plugin_permissions,
                }))
        );
        assert_eq!(status, StatusCode::CREATED, "{role}");
        let member = common::seed_user(&db.pool, email, UserRole::Member).await;
        rustrak::services::RoleService::assign(
            &db.pool,
            member.id,
            role["id"].as_i64().unwrap() as i32,
            Some(project),
        )
        .await
        .unwrap();
        tokens.push(common::token_for(&db.pool, member.id).await);
    }
    let status_of = |route: &str, token: &str| {
        test::TestRequest::get()
            .uri(&format!(
                "/api/projects/{project}/plugins/web-traffic{route}"
            ))
            .insert_header(bearer(token))
    };
    // The report: figures and rankings, nobody in particular.
    for route in [
        "/overview",
        "/top?by=path",
        "/page?path=/",
        "/live",
        "/event-properties?name=x",
        "/event?name=x",
        "/event-series",
        "/event-pages?name=x",
    ] {
        let (status, _) = call!(&app, status_of(route, &tokens[0]));
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{route} without traffic:read"
        );
        let (status, _) = call!(&app, status_of(route, &tokens[1]));
        assert_eq!(status, StatusCode::OK, "{route} with traffic:read");
    }
    // One visitor at a time: their day, their profile, who did an event.
    // Nobody is `x`: those two are found allowed, and not found.
    for (route, allowed) in [
        ("/visitors", StatusCode::OK),
        ("/visitor?id=x", StatusCode::NOT_FOUND),
        ("/journey?id=x", StatusCode::NOT_FOUND),
        ("/event-recent?name=x", StatusCode::OK),
    ] {
        let (status, _) = call!(&app, status_of(route, &tokens[1]));
        assert_eq!(
            status,
            StatusCode::FORBIDDEN,
            "{route} without visitors:read"
        );
        let (status, _) = call!(&app, status_of(route, &tokens[2]));
        assert_eq!(status, allowed, "{route} with visitors:read");
    }
}

#[actix_web::test]
async fn the_tracker_is_served_to_any_website() {
    let (_db, app, _owner, _project, _key) = installed().await;
    let resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/public/web-traffic/script.js")
            .insert_header(("Origin", "https://shop.example.com"))
            .to_request(),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("content-type").unwrap(),
        "text/javascript; charset=utf-8"
    );
    let body = test::read_body(resp).await;
    let script = String::from_utf8_lossy(&body);
    // It reports to the plugin whose /public/<plugin>/ served it.
    assert!(script.contains("/plugins/${plugin}/collect?sentry_key="));
    assert!(!script.contains("document.cookie"), "no cookies, ever");
}

#[actix_web::test]
async fn pageviews_and_events_are_history_the_salt_is_not() {
    let (_db, app, owner, project, key) = installed().await;
    call!(
        &app,
        beacon(
            project,
            &key,
            "10.0.0.1",
            FIREFOX,
            pageview("https://shop.example.com/")
        )
    );
    let (_, storage) = call!(
        &app,
        test::TestRequest::get()
            .uri("/api/storage/plugins")
            .insert_header(bearer(&owner))
    );
    let tables: Vec<&str> = storage[0]["history"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["table"].as_str().unwrap())
        .collect();
    assert_eq!(tables, ["events", "pageviews"], "{storage}");
}
