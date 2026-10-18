use crate::common::TestDb;
use chrono::{DateTime, Utc};
use rustrak::models::{CreateAuthToken, CreateProject, CreateUserRequest, ProjectRole, UserRole};
use rustrak::services::grouping::DenormalizedFields;
use rustrak::services::{
    AuthTokenService, IssueService, ProjectMemberService, ProjectService, UsersService,
};
use serde_json::Value;
use uuid::Uuid;

pub struct Fixture {
    pub db: TestDb,
    pub project: i32,
    pub other: i32,
    pub user: i32,
    pub token: String,
    pub admin_token: String,
    pub issues: [Uuid; 2],
}

impl Fixture {
    pub async fn new() -> Self {
        let db = TestDb::new().await;
        let mut projects = Vec::new();
        for name in ["Identity lookup", "Other project"] {
            projects.push(
                ProjectService::create(
                    &db.pool,
                    CreateProject {
                        name: name.into(),
                        slug: None,
                        platform: None,
                    },
                )
                .await
                .unwrap()
                .id,
            );
        }
        let user = UsersService::create_user(
            &db.pool,
            &CreateUserRequest {
                email: "identity@example.test".into(),
                password: "synthetic-password".into(),
            },
            UserRole::Member,
        )
        .await
        .unwrap()
        .id;
        ProjectMemberService::upsert(&db.pool, projects[0], user, ProjectRole::Viewer)
            .await
            .unwrap();
        let token = AuthTokenService::create_for_user(
            &db.pool,
            CreateAuthToken { description: None },
            Some(user),
        )
        .await
        .unwrap()
        .token;
        let admin_token = AuthTokenService::create(&db.pool, CreateAuthToken { description: None })
            .await
            .unwrap()
            .token;
        let mut issues = Vec::new();
        for message in ["First issue", "Second issue"] {
            let fields = DenormalizedFields {
                calculated_type: "TypeError".into(),
                calculated_value: message.into(),
                transaction: String::new(),
                last_frame_filename: String::new(),
                last_frame_module: String::new(),
                last_frame_function: String::new(),
                culprit: String::new(),
                logger: String::new(),
                release: String::new(),
            };
            issues.push(
                IssueService::create(
                    &db.pool,
                    projects[0],
                    timestamp(),
                    &fields,
                    Some("error"),
                    Some("javascript"),
                )
                .await
                .unwrap()
                .id,
            );
        }
        Self {
            db,
            project: projects[0],
            other: projects[1],
            user,
            token,
            admin_token,
            issues: [issues[0], issues[1]],
        }
    }

    pub async fn insert(&self, project: i32, issue: Option<Uuid>, data: Value) -> Uuid {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO events (id,event_id,project_id,issue_id,data,timestamp,ingested_at) VALUES ($1,$2,$3,$4,$5,$6,$6)")
            .bind(id).bind(Uuid::new_v4()).bind(project).bind(issue).bind(data)
            .bind(timestamp()).execute(&self.db.pool).await.unwrap();
        id
    }
}

pub fn timestamp() -> DateTime<Utc> {
    "2026-09-30T12:00:00Z".parse().unwrap()
}

pub fn uri(project: i32, params: &[(&str, &str)]) -> String {
    let query = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(params.iter().copied())
        .finish();
    format!("/api/projects/{project}/events/lookup?{query}")
}
