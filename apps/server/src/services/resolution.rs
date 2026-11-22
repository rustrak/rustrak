//! Issue resolutions in the database: Sentry's `GroupResolution` rows, the
//! releases events create, and `clear_expired_resolutions`. The decisions
//! are [`crate::models::resolution`]'s; this module reads and writes them.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::error::AppResult;
use crate::models::resolution::{
    status_details, ProjectReleases, ReleaseRef, ResolutionPlan, ResolutionRef, ResolutionType,
    ACTIVITY_RESOLVED_IN_RELEASE, STATUS_PENDING, STATUS_RESOLVED,
};
use crate::services::IssueSocialService;

pub type DbConn = <crate::db::Db as sqlx::Database>::Connection;

/// One `issue_resolutions` row with the releases it names.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StoredResolution {
    pub issue_id: Uuid,
    #[sqlx(rename = "type")]
    pub kind: Option<i16>,
    pub status: i16,
    pub current_release_version: Option<String>,
    #[sqlx(flatten)]
    pub release: StoredRelease,
}

/// The release a resolution is anchored on.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StoredRelease {
    pub release_id: i32,
    pub release_version: String,
    pub release_date_created: DateTime<Utc>,
    pub release_date_released: Option<DateTime<Utc>>,
}

impl StoredResolution {
    pub fn release(&self) -> ReleaseRef {
        ReleaseRef {
            id: self.release.release_id,
            version: self.release.release_version.clone(),
            date_created: self.release.release_date_created,
            date_released: self.release.release_date_released,
        }
    }

    pub fn kind(&self) -> Option<ResolutionType> {
        ResolutionType::from_code(self.kind)
    }
}

/// `has_resolution` for one stored row against `releases` (which must hold
/// the event's release): what the digest asks of every event of a resolved
/// issue.
pub fn holds(
    resolution: Option<&StoredResolution>,
    releases: &ProjectReleases,
    release: &str,
) -> bool {
    let event = (!release.is_empty())
        .then(|| releases.by_version(release))
        .flatten();
    let Some(stored) = resolution else {
        return crate::models::resolution::has_resolution(None, event, false);
    };
    let anchor = stored.release();
    let current_version = stored.current_release_version.as_deref();
    let resolution = ResolutionRef {
        kind: stored.kind(),
        release: &anchor,
        current_release_version: current_version,
        current_release: current_version.and_then(|v| releases.by_version(v)),
    };
    let follows = event.is_some_and(|r| releases.follows_semver(Some(&r.version)));
    crate::models::resolution::has_resolution(Some(&resolution), event, follows)
}

const RESOLUTION_COLUMNS: &str = "r.issue_id, r.type, r.status, r.current_release_version, \
     rel.id AS release_id, rel.version AS release_version, \
     rel.date_created AS release_date_created, rel.date_released AS release_date_released";

pub struct ResolutionService;

impl ResolutionService {
    /// Every release of `project_id`.
    pub async fn project_releases(
        conn: &mut DbConn,
        project_id: i32,
    ) -> AppResult<ProjectReleases> {
        let rows: Vec<ReleaseRef> = sqlx::query_as(
            "SELECT id, version, date_created, date_released FROM releases WHERE project_id = $1",
        )
        .bind(project_id)
        .fetch_all(&mut *conn)
        .await?;
        Ok(ProjectReleases::new(rows))
    }

    /// Sentry's `Release.get_or_create` for the releases events name, each
    /// dated by the first event that names it. Returns those it created,
    /// oldest first; invalid versions are skipped, as Sentry's
    /// `ValidationError` leaves the event without a release.
    pub async fn create_releases(
        conn: &mut DbConn,
        project_id: i32,
        versions: &[(&str, DateTime<Utc>)],
    ) -> AppResult<Vec<ReleaseRef>> {
        let valid: Vec<&(&str, DateTime<Utc>)> = versions
            .iter()
            .filter(|(v, _)| crate::models::is_valid_version(v))
            .collect();
        if valid.is_empty() {
            return Ok(Vec::new());
        }
        let mut insert = sqlx::QueryBuilder::<crate::db::Db>::new(
            "INSERT INTO releases (project_id, version, date_created) ",
        );
        insert.push_values(valid, |mut b, (version, at)| {
            b.push_bind(project_id).push_bind(*version).push_bind(*at);
        });
        insert.push(
            " ON CONFLICT (project_id, version) DO NOTHING \
             RETURNING id, version, date_created, date_released",
        );
        let mut created: Vec<ReleaseRef> = insert
            .build_query_as::<ReleaseRef>()
            .fetch_all(&mut *conn)
            .await?;
        created.sort_by(|a, b| {
            a.date_created
                .cmp(&b.date_created)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(created)
    }

    /// The resolutions of `issue_ids` that exist.
    pub async fn of_issues(
        conn: &mut DbConn,
        issue_ids: &[Uuid],
    ) -> AppResult<HashMap<Uuid, StoredResolution>> {
        if issue_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut select = sqlx::QueryBuilder::<crate::db::Db>::new(format!(
            "SELECT {RESOLUTION_COLUMNS} FROM issue_resolutions r \
             JOIN releases rel ON rel.id = r.release_id WHERE r.issue_id IN ("
        ));
        let mut ids = select.separated(", ");
        for id in issue_ids {
            ids.push_bind(*id);
        }
        select.push(")");
        let rows: Vec<StoredResolution> = select
            .build_query_as::<StoredResolution>()
            .fetch_all(&mut *conn)
            .await?;
        Ok(rows.into_iter().map(|r| (r.issue_id, r)).collect())
    }

    /// Writes `plan` as the issue's resolution, replacing any other, and the
    /// `statusDetails` it shows.
    pub async fn write(
        conn: &mut DbConn,
        issue_id: Uuid,
        project_id: i32,
        plan: &ResolutionPlan,
        actor_id: Option<i32>,
    ) -> AppResult<()> {
        sqlx::query(
            r#"
            INSERT INTO issue_resolutions
                (issue_id, project_id, release_id, type, status, current_release_version, actor_id, datetime)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (issue_id) DO UPDATE SET
                release_id = excluded.release_id,
                type = excluded.type,
                status = excluded.status,
                current_release_version = excluded.current_release_version,
                actor_id = excluded.actor_id,
                datetime = excluded.datetime
            "#,
        )
        .bind(issue_id)
        .bind(project_id)
        .bind(plan.release.id)
        .bind(plan.kind.code())
        .bind(plan.status)
        .bind(&plan.current_release_version)
        .bind(actor_id)
        .bind(Utc::now())
        .execute(&mut *conn)
        .await?;
        sqlx::query("UPDATE issues SET status_details = $2 WHERE id = $1")
            .bind(issue_id)
            .bind(status_details(Some(plan.kind), &plan.release.version))
            .execute(&mut *conn)
            .await?;
        Ok(())
    }

    /// Removes the resolutions of `issue_ids`: the issue left `resolved`.
    pub async fn delete(conn: &mut DbConn, issue_ids: &[Uuid]) -> AppResult<()> {
        if issue_ids.is_empty() {
            return Ok(());
        }
        let mut delete = sqlx::QueryBuilder::<crate::db::Db>::new(
            "DELETE FROM issue_resolutions WHERE issue_id IN (",
        );
        let mut ids = delete.separated(", ");
        for id in issue_ids {
            ids.push_bind(*id);
        }
        delete.push(")");
        delete.build().execute(&mut *conn).await?;
        Ok(())
    }

    /// Sentry's `clear_expired_resolutions` for a release just created: every
    /// pending in-next-release resolution of the project anchored on an
    /// older release is now resolved in this one, and its
    /// "resolved in release" activity names it. Returns how many.
    pub async fn clear_expired(
        conn: &mut DbConn,
        project_id: i32,
        release: &ReleaseRef,
    ) -> AppResult<u64> {
        // One statement, so a resolution removed meanwhile (the issue was
        // reopened) is not resolved again: only the rows it changed come back.
        let resolved: Vec<(Uuid,)> = sqlx::query_as(
            r#"
            UPDATE issue_resolutions SET release_id = $5, type = $6, status = $7
            WHERE project_id = $1 AND status = $2
              AND (type IS NULL OR type = $3)
              AND release_id <> $5
              AND release_id IN (
                  SELECT id FROM releases WHERE project_id = $1 AND date_created < $4
              )
            RETURNING issue_id
            "#,
        )
        .bind(project_id)
        .bind(STATUS_PENDING)
        .bind(ResolutionType::InNextRelease.code())
        .bind(release.date_created)
        .bind(release.id)
        .bind(ResolutionType::InRelease.code())
        .bind(STATUS_RESOLVED)
        .fetch_all(&mut *conn)
        .await?;
        let details = status_details(Some(ResolutionType::InRelease), &release.version);
        for (issue_id,) in &resolved {
            sqlx::query("UPDATE issues SET status_details = $2 WHERE id = $1")
                .bind(issue_id)
                .bind(&details)
                .execute(&mut *conn)
                .await?;
            Self::set_activity_version(conn, *issue_id, &release.version, true).await?;
        }
        Ok(resolved.len() as u64)
    }

    /// Sets `version` on the issue's latest "resolved in release" activity,
    /// always (`_update_resolution_activities`) or only where it has none
    /// yet (a regression, `_handle_regression`). Returns the version the
    /// activity ends with, if there is one.
    pub async fn set_activity_version(
        conn: &mut DbConn,
        issue_id: Uuid,
        version: &str,
        always: bool,
    ) -> AppResult<Option<String>> {
        let latest: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT id, data FROM issue_activity WHERE issue_id = $1 AND type = $2 \
             ORDER BY created_at DESC, id DESC LIMIT 1",
        )
        .bind(issue_id)
        .bind(ACTIVITY_RESOLVED_IN_RELEASE)
        .fetch_optional(&mut *conn)
        .await?;
        let Some((id, data)) = latest else {
            return Ok(None);
        };
        let mut data: serde_json::Value =
            serde_json::from_str(&data).unwrap_or_else(|_| serde_json::json!({}));
        if !data.is_object() {
            data = serde_json::json!({});
        }
        let current = data.get("version").and_then(|v| v.as_str());
        if always || current.is_none_or(str::is_empty) {
            data["version"] = version.into();
            sqlx::query("UPDATE issue_activity SET data = $2 WHERE id = $1")
                .bind(id)
                .bind(data.to_string())
                .execute(&mut *conn)
                .await?;
        }
        Ok(data
            .get("version")
            .and_then(|v| v.as_str())
            .map(str::to_string))
    }

    /// The "resolved in release" activity of a resolution.
    pub async fn record_activity(
        conn: &mut DbConn,
        issue_id: Uuid,
        user_id: Option<i32>,
        data: &serde_json::Value,
    ) -> AppResult<()> {
        IssueSocialService::add_activity_on(
            conn,
            issue_id,
            user_id,
            ACTIVITY_RESOLVED_IN_RELEASE,
            &data.to_string(),
        )
        .await
    }
}
