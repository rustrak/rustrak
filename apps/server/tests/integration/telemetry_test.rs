//! Anonymous telemetry: the PostHog adapter over real HTTP, the instance id
//! in the database, the volume queries and the reporter loop.

use std::sync::{Arc, Mutex};

use actix_web::{web, App, HttpResponse, HttpServer};
use serde_json::Value;

use crate::common::telemetry::sample_report;
use rustrak::telemetry::posthog::PostHogSink;
use rustrak::telemetry::{Sink, SinkError};

/// A local collector: records every JSON body it receives and answers with
/// one fixed status.
struct Collector {
    url: String,
    received: Arc<Mutex<Vec<Value>>>,
}

impl Collector {
    async fn start(status: u16) -> Self {
        Self::start_with(status, false).await
    }

    /// Answers every request with a 307 back to itself.
    async fn start_redirecting() -> Self {
        Self::start_with(307, true).await
    }

    async fn start_with(status: u16, redirect_to_self: bool) -> Self {
        let received: Arc<Mutex<Vec<Value>>> = Arc::default();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind collector");
        let addr = listener.local_addr().expect("collector addr");
        let sink = received.clone();
        let server = HttpServer::new(move || {
            let sink = sink.clone();
            App::new().default_service(web::to(move |body: web::Json<Value>| {
                let sink = sink.clone();
                async move {
                    sink.lock().unwrap().push(body.into_inner());
                    let mut response = HttpResponse::build(
                        actix_web::http::StatusCode::from_u16(status).expect("valid status"),
                    );
                    if redirect_to_self {
                        response.insert_header(("Location", format!("http://{addr}/i/v0/e/")));
                    }
                    response.finish()
                }
            }))
        })
        .workers(1)
        .listen(listener)
        .expect("listen")
        .run();
        tokio::spawn(server);
        Self {
            url: format!("http://{addr}/i/v0/e/"),
            received,
        }
    }

    fn received(&self) -> Vec<Value> {
        self.received.lock().unwrap().clone()
    }
}

#[actix_web::test]
async fn the_posthog_sink_posts_one_heartbeat_to_its_endpoint() {
    let collector = Collector::start(200).await;
    let sink = PostHogSink::new(&collector.url, "phc_test");

    sink.send(&sample_report())
        .await
        .expect("a 200 is a delivery");

    let received = collector.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0]["event"], "heartbeat");
    assert_eq!(received[0]["api_key"], "phc_test");
    assert_eq!(received[0]["properties"]["version"], "0.15.0");
}

#[actix_web::test]
async fn a_rejected_heartbeat_reports_the_status_and_is_not_retried() {
    let collector = Collector::start(401).await;
    let sink = PostHogSink::new(&collector.url, "phc_test");

    let error = sink
        .send(&sample_report())
        .await
        .expect_err("401 is a failure");

    assert!(matches!(error, SinkError::Status(401)), "{error:?}");
    assert_eq!(collector.received().len(), 1);
}

#[actix_web::test]
async fn an_unreachable_sink_is_a_transport_error() {
    // A port the kernel just handed out and that nothing listens on any more.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .expect("bind")
        .local_addr()
        .expect("addr")
        .port();
    let sink = PostHogSink::new(format!("http://127.0.0.1:{port}/i/v0/e/"), "phc_test");
    let error = sink
        .send(&sample_report())
        .await
        .expect_err("nothing listens");
    assert!(matches!(error, SinkError::Transport(_)), "{error:?}");
}

/// The endpoint is fixed and HTTPS. A redirect is not followed, so the key
/// and the report can never be re-posted somewhere else by a 307.
#[actix_web::test]
async fn a_redirect_is_not_followed() {
    let collector = Collector::start_redirecting().await;
    let sink = PostHogSink::new(&collector.url, "phc_test");

    let error = sink
        .send(&sample_report())
        .await
        .expect_err("a 307 is not a delivery");

    assert!(matches!(error, SinkError::Status(307)), "{error:?}");
    assert_eq!(
        collector.received().len(),
        1,
        "posted once, never re-posted"
    );
}

mod instance_id {
    use crate::common::TestDb;
    use rustrak::telemetry::instance_id;

    #[tokio::test]
    async fn is_a_random_uuid_created_once_and_kept() {
        let db = TestDb::new().await;
        let first = instance_id(&db.pool).await.expect("first read");
        let second = instance_id(&db.pool).await.expect("second read");

        let parsed = uuid::Uuid::parse_str(&first).expect("a uuid");
        assert_eq!(parsed.get_version(), Some(uuid::Version::Random));
        assert_eq!(first, second, "the id survives across calls");
    }

    #[tokio::test]
    async fn two_installations_never_share_one() {
        let a = TestDb::new().await;
        let b = TestDb::new().await;
        assert_ne!(
            instance_id(&a.pool).await.expect("a"),
            instance_id(&b.pool).await.expect("b")
        );
    }
}

mod volume {
    use crate::common::TestDb;
    use rustrak::models::{CreateProject, CreateUserRequest, UserRole};
    use rustrak::services::{ProjectService, UsersService};
    use rustrak::telemetry::Volume;

    #[tokio::test]
    async fn an_empty_installation_is_all_zeros() {
        let db = TestDb::new().await;
        assert_eq!(
            Volume::collect(&db.pool).await.expect("collect"),
            Volume::default()
        );
    }

    #[tokio::test]
    async fn counts_what_exists_and_blurs_past_two_digits() {
        let db = TestDb::new().await;
        for i in 0..101 {
            ProjectService::create(
                &db.pool,
                CreateProject {
                    name: format!("p{i}"),
                    slug: None,
                    platform: None,
                },
            )
            .await
            .expect("project");
        }
        for i in 0..3 {
            UsersService::create_user(
                &db.pool,
                &CreateUserRequest {
                    email: format!("u{i}@example.com"),
                    password: "password123".to_string(),
                },
                UserRole::Member,
            )
            .await
            .expect("user");
        }

        let volume = Volume::collect(&db.pool).await.expect("collect");
        assert_eq!(volume.projects, 100, "101 blurs to 100");
        assert_eq!(volume.users, 3);
        assert_eq!(volume.events_24h, 0);
    }

    /// How many projects use GitHub, as counts: never a repository's name.
    #[tokio::test]
    async fn counts_the_projects_connected_to_github() {
        let db = TestDb::new().await;
        sqlx::query("INSERT INTO github_installations (id, account_login) VALUES (1, 'acme')")
            .execute(&db.pool)
            .await
            .expect("installation");
        for (i, auto_create) in [true, false, false].into_iter().enumerate() {
            let project = ProjectService::create(
                &db.pool,
                CreateProject {
                    name: format!("p{i}"),
                    slug: None,
                    platform: None,
                },
            )
            .await
            .expect("project");
            sqlx::query(
                "INSERT INTO project_repositories (project_id, installation_id, repo_full_name, auto_create_issues)
                 VALUES ($1, 1, 'acme/web', $2)",
            )
            .bind(project.id)
            .bind(auto_create)
            .execute(&db.pool)
            .await
            .expect("repository");
        }
        // One more project, not connected.
        ProjectService::create(
            &db.pool,
            CreateProject {
                name: "alone".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .expect("project");

        let volume = Volume::collect(&db.pool).await.expect("collect");
        assert_eq!(volume.github_projects, 3);
        assert_eq!(volume.github_auto_create_projects, 1);
    }

    #[tokio::test]
    async fn github_connected_is_whether_an_app_is() {
        use rustrak::telemetry::github_connected;

        let db = TestDb::new().await;
        assert!(!github_connected(&db.pool).await.expect("none"));
        sqlx::query(
            "INSERT INTO github_app (id, app_id, slug, client_id, client_secret, webhook_secret,
                                     private_key, html_url, web_url, api_url)
             VALUES (1, 7, 'rustrak', 'c', 's', 'w', 'k', 'h', 'https://github.com', 'https://api.github.com')",
        )
        .execute(&db.pool)
        .await
        .expect("app");
        assert!(github_connected(&db.pool).await.expect("one"));
    }

    /// The kinds of alert channel in use, as kinds: never a name, never a URL.
    #[tokio::test]
    async fn alert_providers_are_the_enabled_kinds_deduplicated() {
        use rustrak::models::{CreateAlertIntegration, ProviderType};
        use rustrak::services::AlertService;
        use rustrak::telemetry::alert_providers;
        use serde_json::json;

        let db = TestDb::new().await;
        assert!(alert_providers(&db.pool).await.expect("none").is_empty());
        let webhook = json!({ "url": "http://127.0.0.1:9/hook" });
        let slack = json!({
            "method": "webhook",
            "webhook_url": "https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXX"
        });
        for (name, kind, credentials, enabled) in [
            ("a", ProviderType::Webhook, webhook.clone(), true),
            ("b", ProviderType::Webhook, webhook, true),
            ("c", ProviderType::Slack, slack, true),
            (
                "d",
                ProviderType::Email,
                json!({ "smtp_host": "smtp.example.com", "from_address": "a@example.com" }),
                false,
            ),
        ] {
            AlertService::create_channel(
                &db.pool,
                &Default::default(),
                CreateAlertIntegration {
                    name: name.to_string(),
                    provider_type: kind,
                    credentials,
                    is_enabled: enabled,
                },
            )
            .await
            .expect("channel");
        }
        assert_eq!(
            alert_providers(&db.pool).await.expect("kinds"),
            vec!["slack".to_string(), "webhook".to_string()]
        );
    }
}

mod reporter {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use async_trait::async_trait;

    use crate::common::TestDb;
    use rustrak::telemetry::{
        instance_id, ConfigFacts, Context, Counters, Report, Reporter, Schedule, Sink, SinkError,
    };

    /// Keeps every report; fails the first `fail_first` sends.
    #[derive(Default)]
    struct Memory {
        reports: Mutex<Vec<Report>>,
        fail_first: Mutex<usize>,
    }

    #[async_trait]
    impl Sink for Memory {
        fn name(&self) -> &'static str {
            "memory"
        }
        async fn send(&self, report: &Report) -> Result<(), SinkError> {
            let mut left = self.fail_first.lock().unwrap();
            if *left > 0 {
                *left -= 1;
                return Err(SinkError::Transport("down".into()));
            }
            self.reports.lock().unwrap().push(report.clone());
            Ok(())
        }
    }

    fn leaked_counters() -> &'static Counters {
        Box::leak(Box::new(Counters::new()))
    }

    fn context() -> Context {
        Context {
            dashboard_served: true,
            config: ConfigFacts {
                ssl_proxy: false,
                public_url_set: true,
                smtp_configured: false,
                session_secret_set: true,
                alert_providers: vec![],
                github_connected: false,
                quota_customized: false,
            },
            sqlite_path: None,
            ingest_dir: std::env::temp_dir(),
        }
    }

    #[tokio::test]
    async fn a_report_carries_the_build_the_instance_and_the_drained_counters() {
        let db = TestDb::new().await;
        let counters = leaked_counters();
        counters.digest_ok();
        counters.digest_ok();
        rustrak::services::AlertService::create_channel(
            &db.pool,
            &Default::default(),
            rustrak::models::CreateAlertIntegration {
                name: "hook".to_string(),
                provider_type: rustrak::models::ProviderType::Webhook,
                credentials: serde_json::json!({ "url": "http://127.0.0.1:9/hook" }),
                is_enabled: true,
            },
        )
        .await
        .expect("channel");
        let reporter = Reporter::new(
            db.pool.clone(),
            Arc::new(Memory::default()),
            counters,
            context(),
        );

        let report = reporter.build_report().await.expect("build");

        assert_eq!(report.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(report.os, std::env::consts::OS);
        assert_eq!(report.arch, std::env::consts::ARCH);
        assert_eq!(report.instance_id, instance_id(&db.pool).await.expect("id"));
        assert!(
            report.db_version.is_some(),
            "the engine version is read from the pool"
        );
        assert!(report.cpu_count >= 1);
        assert!(report.dashboard_served);
        assert!(report.config.public_url_set);
        assert_eq!(
            report.config.alert_providers,
            vec!["webhook".to_string()],
            "read at report time"
        );
        assert_eq!(report.health.digest.ok, 2);

        let next = reporter.build_report().await.expect("build again");
        assert_eq!(next.health.digest.ok, 0, "the window was drained");
        assert!(!next.first_since_boot);
    }

    #[tokio::test]
    async fn the_loop_sends_after_the_initial_delay_and_then_every_interval() {
        let db = TestDb::new().await;
        let sink = Arc::new(Memory::default());
        let reporter = Reporter::new(db.pool.clone(), sink.clone(), leaked_counters(), context())
            .with_schedule(Schedule {
                initial_delay: Duration::from_millis(50),
                interval: Duration::from_millis(100),
                sample_every: Duration::from_millis(10),
            });
        tokio::spawn(Arc::new(reporter).run());

        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(
            sink.reports.lock().unwrap().is_empty(),
            "nothing before the delay"
        );

        let reports = crate::common::eventually(|| async {
            let reports = sink.reports.lock().unwrap().clone();
            (reports.len() >= 2).then_some(reports)
        })
        .await;
        assert!(reports[0].first_since_boot);
        assert!(!reports[1].first_since_boot);
        assert!(reports[1].uptime_secs >= reports[0].uptime_secs);
    }

    /// A miss waits for the next tick; nothing is retried and nothing stops.
    #[tokio::test]
    async fn a_failed_send_does_not_stop_the_loop() {
        let db = TestDb::new().await;
        let sink = Arc::new(Memory {
            fail_first: Mutex::new(1),
            ..Default::default()
        });
        let reporter = Reporter::new(db.pool.clone(), sink.clone(), leaked_counters(), context())
            .with_schedule(Schedule {
                initial_delay: Duration::from_millis(10),
                interval: Duration::from_millis(50),
                sample_every: Duration::from_millis(10),
            });
        tokio::spawn(Arc::new(reporter).run());

        let reports = crate::common::eventually(|| async {
            let reports = sink.reports.lock().unwrap().clone();
            (!reports.is_empty()).then_some(reports)
        })
        .await;
        assert!(
            !reports[0].first_since_boot,
            "the lost first report is not replayed"
        );
    }
}

mod middleware {
    use actix_web::{http::StatusCode, test, web, App, HttpResponse};
    use rustrak::middleware::telemetry::TelemetryMiddleware;
    use rustrak::telemetry::Counters;

    fn leaked_counters() -> &'static Counters {
        Box::leak(Box::new(Counters::new()))
    }

    async fn answer(req: actix_web::HttpRequest) -> HttpResponse {
        let status: u16 = req.match_info().query("status").parse().unwrap();
        HttpResponse::build(StatusCode::from_u16(status).unwrap()).finish()
    }

    async fn drive(counters: &'static Counters, requests: &[&str]) {
        let app = test::init_service(
            App::new()
                .wrap(TelemetryMiddleware::new(counters))
                .route(
                    "/api/{project_id}/envelope/{status}",
                    web::post().to(answer),
                )
                .route("/api/{project_id}/store/{status}", web::post().to(answer))
                .route("/api/issues/{id}/{status}", web::get().to(answer))
                .route("/api/projects/{status}", web::get().to(answer)),
        )
        .await;
        for uri in requests {
            let req = if uri.contains("/envelope/") || uri.contains("/store/") {
                test::TestRequest::post().uri(uri).to_request()
            } else {
                test::TestRequest::get().uri(uri).to_request()
            };
            let _ = test::call_service(&app, req).await;
        }
    }

    #[actix_web::test]
    async fn ingest_outcomes_are_bucketed_by_status() {
        let counters = leaked_counters();
        drive(
            counters,
            &[
                "/api/1/envelope/200",
                "/api/1/store/200",
                "/api/1/envelope/429",
                "/api/1/envelope/401",
                "/api/1/envelope/403",
                "/api/1/envelope/413",
                "/api/1/envelope/400",
                "/api/1/envelope/418",
            ],
        )
        .await;
        let ingest = counters.snapshot_and_reset().ingest;
        assert_eq!(ingest.accepted, 2);
        assert!(ingest.latency_ms.p50.is_some());
        let r = ingest.rejected;
        assert_eq!(
            (r.rate_limit, r.auth, r.too_large, r.malformed, r.other),
            (1, 2, 1, 1, 1)
        );
    }

    /// Reads the whole body, then marks the server's own work as started, the
    /// way the ingest handler does.
    async fn answer_after_body(req: actix_web::HttpRequest, _body: web::Bytes) -> HttpResponse {
        rustrak::middleware::telemetry::mark_body_received(&req);
        HttpResponse::Ok().finish()
    }

    /// A client on a slow link takes its time uploading the envelope. That is
    /// the network, not the server, and must not show up as ingest latency:
    /// the heartbeat's p50/p99 exist to spot a slow server.
    #[actix_web::test]
    async fn a_slow_upload_is_not_reported_as_server_latency() {
        let counters = leaked_counters();
        let app = test::init_service(App::new().wrap(TelemetryMiddleware::new(counters)).route(
            "/api/{project_id}/envelope/",
            web::post().to(answer_after_body),
        ))
        .await;
        let slow_body = futures_util::stream::once(async {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            Ok::<_, actix_web::error::PayloadError>(web::Bytes::from_static(b"{}"))
        });
        let (req, _) = test::TestRequest::post()
            .uri("/api/1/envelope/")
            .to_request()
            .replace_payload(actix_web::dev::Payload::Stream {
                payload: Box::pin(slow_body)
                    as std::pin::Pin<Box<dyn futures_util::Stream<Item = _>>>,
            });

        let res = test::call_service(&app, req).await;

        assert!(res.status().is_success());
        let latency = counters.snapshot_and_reset().ingest.latency_ms;
        assert!(
            latency.p50.is_some_and(|ms| ms < 100),
            "the upload's 300ms was counted as server time: {latency:?}"
        );
    }

    /// A 5xx caused by the database also counts the error's code, the one
    /// signal that tells a failing constraint from a failing connection.
    #[actix_web::test]
    async fn a_database_error_behind_a_5xx_is_counted_by_its_code() {
        let db = crate::common::TestDb::new().await;
        let error = sqlx::query("SELECT * FROM no_such_table")
            .execute(&db.pool)
            .await
            .unwrap_err();
        let code = error
            .as_database_error()
            .and_then(|e| e.code())
            .expect("a database error carries a code")
            .into_owned();
        let count = || {
            Counters::global()
                .snapshot()
                .db_errors_by_code
                .get(&code)
                .copied()
                .unwrap_or(0)
        };
        let before = count();

        let response =
            actix_web::ResponseError::error_response(&rustrak::error::AppError::from(error));

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(count() > before, "code {code} was not counted");
    }

    #[actix_web::test]
    async fn server_errors_are_counted_by_route_pattern_not_path() {
        let counters = leaked_counters();
        drive(
            counters,
            &[
                "/api/issues/17/500",
                "/api/issues/99/503",
                "/api/projects/500",
                "/api/1/envelope/500",
            ],
        )
        .await;
        let health = counters.snapshot_and_reset();
        assert_eq!(health.http_5xx_by_route["/api/issues/{id}/{status}"], 2);
        assert_eq!(health.http_5xx_by_route["/api/projects/{status}"], 1);
        assert_eq!(
            health.http_5xx_by_route["/api/{project_id}/envelope/{status}"],
            1
        );
        assert!(health.http_5xx_by_route.keys().all(|k| !k.contains("17")));
        assert_eq!(
            health.ingest.accepted, 0,
            "a 500 on ingest is not an accept"
        );
        assert_eq!(health.ingest.rejected.other, 0, "nor a rejection");
    }

    #[actix_web::test]
    async fn ordinary_api_traffic_is_not_counted_at_all() {
        let counters = leaked_counters();
        drive(counters, &["/api/projects/200", "/api/issues/1/404"]).await;
        let health = counters.snapshot_and_reset();
        assert_eq!(health.ingest.accepted, 0);
        assert_eq!(health.ingest.rejected.other, 0);
        assert!(health.http_5xx_by_route.is_empty());
    }
}

mod digest {
    use chrono::Utc;
    use serde_json::json;
    use tempfile::TempDir;
    use uuid::Uuid;

    use crate::common::TestDb;

    use rustrak::digest::processors::Processors;
    use rustrak::ingest::pipeline::digest_stored_event;
    use rustrak::ingest::EventMetadata;
    use rustrak::models::CreateProject;
    use rustrak::services::ProjectService;
    use rustrak::telemetry::Counters;

    fn leaked_counters() -> &'static Counters {
        Box::leak(Box::new(Counters::new()))
    }

    async fn setup(
        pool: &rustrak::db::DbPool,
        ingest_dir: &std::path::Path,
    ) -> (Processors, EventMetadata) {
        let project = ProjectService::create(
            pool,
            CreateProject {
                name: "Telemetry".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .expect("project");
        let processors = crate::common::processors_with_config(
            pool,
            &rustrak::config::Config {
                ingest_dir: ingest_dir.to_path_buf(),
                rate_limit: crate::common::rate_limit(),
                ..crate::common::config()
            },
        )
        .with_counters(leaked_counters());
        let event_id = Uuid::new_v4().to_string().replace('-', "");
        let metadata = EventMetadata {
            event_id,
            project_id: project.id,
            ingested_at: Utc::now(),
            remote_addr: None,
        };
        (processors, metadata)
    }

    #[actix_web::test]
    async fn a_digested_event_counts_as_ok() {
        let db = TestDb::new().await;
        let dir = TempDir::new().expect("temp dir");
        let (processors, metadata) = setup(&db.pool, dir.path()).await;
        let event = json!({
            "event_id": metadata.event_id,
            "timestamp": Utc::now().timestamp() as f64,
            "platform": "rust",
            "exception": { "values": [{ "type": "TypeError", "value": "boom" }] }
        });
        crate::common::store_event(
            dir.path(),
            metadata.project_id,
            &metadata.event_id,
            &serde_json::to_vec(&event).unwrap(),
        )
        .await
        .expect("store");

        digest_stored_event(&processors, &db.pool, &metadata).await;

        let digest = processors.counters().snapshot_and_reset().digest;
        assert_eq!((digest.ok, digest.failed), (1, 0));
    }

    #[actix_web::test]
    async fn an_event_that_cannot_be_digested_counts_as_failed() {
        let db = TestDb::new().await;
        let dir = TempDir::new().expect("temp dir");
        let (processors, metadata) = setup(&db.pool, dir.path()).await;
        // Nothing was stored under this id: the digest has nothing to read.

        digest_stored_event(&processors, &db.pool, &metadata).await;

        let digest = processors.counters().snapshot_and_reset().digest;
        assert_eq!((digest.ok, digest.failed), (0, 1));
    }
}

mod alerts {

    use chrono::Utc;
    use serde_json::json;

    use super::Collector;
    use crate::common::TestDb;
    use rustrak::models::{
        AlertRuleChannelInput, AlertType, CreateAlertIntegration, CreateAlertRule, CreateProject,
        ProviderType,
    };
    use rustrak::services::grouping::DenormalizedFields;
    use rustrak::services::{AlertService, IssueService, ProjectService};
    use rustrak::telemetry::Counters;

    /// Every dispatcher is a static method with no instance to hand a
    /// counter set to, so this is the one path that counts on the global set.
    #[actix_web::test]
    async fn a_failed_delivery_is_counted_under_its_provider_kind() {
        let db = TestDb::new().await;
        let collector = Collector::start(500).await;
        let project = ProjectService::create(
            &db.pool,
            CreateProject {
                name: "Alerts".to_string(),
                slug: None,
                platform: None,
            },
        )
        .await
        .expect("project");
        let channel = AlertService::create_channel(
            &db.pool,
            &Default::default(),
            CreateAlertIntegration {
                name: "Broken webhook".to_string(),
                provider_type: ProviderType::Webhook,
                credentials: json!({ "url": collector.url }),
                is_enabled: true,
            },
        )
        .await
        .expect("channel");
        AlertService::create_rule(
            &db.pool,
            project.id,
            CreateAlertRule {
                name: "Any new issue".to_string(),
                alert_type: AlertType::NewIssue,
                channels: vec![AlertRuleChannelInput {
                    integration_id: channel.id,
                    routing_override: json!({}),
                }],
                conditions: json!({}),
                cooldown_minutes: 0,
            },
        )
        .await
        .expect("rule");
        let issue = IssueService::create(
            &db.pool,
            project.id,
            Utc::now(),
            &DenormalizedFields {
                calculated_type: "Error".to_string(),
                calculated_value: "broken webhook".to_string(),
                transaction: "/test".to_string(),
                last_frame_filename: "test.rs".to_string(),
                last_frame_module: "test".to_string(),
                last_frame_function: "test".to_string(),
                culprit: "test".to_string(),
                logger: String::new(),
                release: String::new(),
            },
            Some("error"),
            Some("rust"),
        )
        .await
        .expect("issue");

        // The counters are process-wide and other tests read them, so they
        // are compared with a baseline and never reset here.
        let failed = || {
            Counters::global()
                .snapshot()
                .alerts_failed_by_provider
                .get("webhook")
                .copied()
                .unwrap_or(0)
        };
        let before = failed();

        AlertService::trigger_event_alert(
            &db.pool,
            &project,
            &issue,
            AlertType::NewIssue,
            uuid::Uuid::new_v4(),
            Default::default(),
            "http://localhost:3000",
            &Default::default(),
        )
        .await
        .expect("trigger");

        crate::common::eventually(|| async {
            (failed() > before && !collector.received().is_empty()).then_some(())
        })
        .await;
        assert_eq!(
            collector.received().len(),
            1,
            "the delivery was attempted once"
        );
    }
}

mod preview {
    use std::sync::Arc;

    use actix_web::{test, web};

    use crate::common::TestDb;
    use rustrak::db::DbPool;
    use rustrak::models::{CreateAuthToken, CreateUserRequest, UserRole};

    use rustrak::services::{AuthTokenService, UsersService};
    use rustrak::telemetry::{
        ConfigFacts, Context, Counters, Disabled, Reporter, Sink, SinkError, TelemetryStatus,
    };

    struct Nowhere;

    #[async_trait::async_trait]
    impl Sink for Nowhere {
        fn name(&self) -> &'static str {
            "nowhere"
        }
        async fn send(&self, _: &rustrak::telemetry::Report) -> Result<(), SinkError> {
            Ok(())
        }
    }

    fn reporter(pool: &DbPool, counters: &'static Counters) -> Arc<Reporter> {
        Arc::new(Reporter::new(
            pool.clone(),
            Arc::new(Nowhere),
            counters,
            Context {
                dashboard_served: false,
                config: ConfigFacts::default(),
                sqlite_path: None,
                ingest_dir: std::env::temp_dir(),
            },
        ))
    }

    async fn token_for(pool: &DbPool, role: UserRole, email: &str) -> String {
        let user = UsersService::create_user(
            pool,
            &CreateUserRequest {
                email: email.to_string(),
                password: "password123".to_string(),
            },
            role,
        )
        .await
        .expect("user");
        AuthTokenService::create_for_user(pool, CreateAuthToken::default(), Some(user.id))
            .await
            .expect("token")
            .token
    }

    async fn preview_app(
        pool: &DbPool,
        status: TelemetryStatus,
        reporter: std::sync::Arc<rustrak::telemetry::Reporter>,
    ) -> impl actix_web::dev::Service<
        actix_http::Request,
        Response = actix_web::dev::ServiceResponse,
        Error = actix_web::Error,
    > {
        let mut state = crate::common::state(pool, crate::common::config()).await;
        state.telemetry_status = web::Data::new(status);
        state.telemetry_reporter = web::Data::new(reporter);
        crate::common::app_with(&state).await
    }

    #[actix_web::test]
    async fn an_admin_sees_exactly_what_the_next_heartbeat_would_carry() {
        let db = TestDb::new().await;
        let counters: &'static Counters = Box::leak(Box::new(Counters::new()));
        counters.digest_ok();
        let reporter = reporter(&db.pool, counters);
        let app = preview_app(
            &db.pool,
            TelemetryStatus::Enabled { sink: "somewhere" },
            reporter.clone(),
        )
        .await;
        let token = token_for(&db.pool, UserRole::Admin, "admin@x.com").await;

        let req = test::TestRequest::get()
            .uri("/api/telemetry/preview")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let res = test::call_service(&app, req).await;
        assert_eq!(res.status(), 200);
        let body: serde_json::Value = test::read_body_json(res).await;
        assert_eq!(body["enabled"], true);
        assert_eq!(body["sink"], "somewhere");
        assert_eq!(body["report"]["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(body["report"]["health"]["digest"]["ok"], 1);
        assert!(body["report"]["instance_id"].is_string());

        // Looking is not taking: the window is still whole for the real send.
        assert_eq!(
            reporter
                .build_report()
                .await
                .expect("build")
                .health
                .digest
                .ok,
            1
        );
    }

    #[actix_web::test]
    async fn the_preview_says_why_nothing_is_sent() {
        let db = TestDb::new().await;
        let counters: &'static Counters = Box::leak(Box::new(Counters::new()));
        let app = preview_app(
            &db.pool,
            TelemetryStatus::Disabled(Disabled::DoNotTrack),
            reporter(&db.pool, counters),
        )
        .await;
        let token = token_for(&db.pool, UserRole::Admin, "admin@x.com").await;

        let req = test::TestRequest::get()
            .uri("/api/telemetry/preview")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        let body: serde_json::Value = test::call_and_read_body_json(&app, req).await;
        assert_eq!(body["enabled"], false);
        assert_eq!(body["reason"], "DO_NOT_TRACK is set");
        assert!(body["sink"].is_null());
        assert!(
            body["report"].is_object(),
            "what would be sent is still shown"
        );
    }

    #[actix_web::test]
    async fn the_preview_is_for_admins_only() {
        let db = TestDb::new().await;
        let counters: &'static Counters = Box::leak(Box::new(Counters::new()));
        let app = preview_app(
            &db.pool,
            TelemetryStatus::Enabled { sink: "somewhere" },
            reporter(&db.pool, counters),
        )
        .await;
        let token = token_for(&db.pool, UserRole::Member, "member@x.com").await;

        let req = test::TestRequest::get()
            .uri("/api/telemetry/preview")
            .insert_header(("Authorization", format!("Bearer {token}")))
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 403);

        let req = test::TestRequest::get()
            .uri("/api/telemetry/preview")
            .to_request();
        assert_eq!(test::call_service(&app, req).await.status(), 401);
    }
}
