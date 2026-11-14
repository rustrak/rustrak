use slug::slugify;

use crate::db::DbPool;
use crate::error::{AppError, AppResult, FieldErrorCode};
use crate::models::{
    CreateProject, Product, Project, QuotaWindows, UpdateProject, SELECTABLE_PLATFORMS,
    VALID_PLATFORMS,
};
use crate::pagination::SortOrder;

/// Whether [`ProjectService::update_inner`] runs the slug availability
/// pre-check before the UPDATE.
///
/// `Skip` is only ever constructed by the test seam
/// [`ProjectService::update_with_raced_slug`], so a release build has no way
/// to reach it.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(feature = "test-seams"), allow(dead_code))]
enum SlugPrecheck {
    Run,
    Skip,
}

pub struct ProjectService;

impl ProjectService {
    pub async fn list(pool: &DbPool) -> AppResult<Vec<Project>> {
        let projects = sqlx::query_as::<_, Project>(
            r#"
            SELECT id, name, slug, sentry_key, stored_event_count,
                   digested_event_count, created_at, updated_at, platform,
                   quota_exceeded_until, quota_exceeded_reason, next_quota_check,
                   quota_minute_window, quota_minute_count, quota_hour_window, quota_hour_count,
                   rate_limited_event_count, rate_limit_per_minute, rate_limit_per_hour,
                   disabled_products, product_order
            FROM projects
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(pool)
        .await?;

        Ok(projects)
    }

    /// Every project's slug by id, for a list across projects to name each
    /// row's project.
    pub async fn slugs(pool: &DbPool) -> AppResult<std::collections::HashMap<i32, String>> {
        let rows: Vec<(i32, String)> = sqlx::query_as("SELECT id, slug FROM projects")
            .fetch_all(pool)
            .await?;
        Ok(rows.into_iter().collect())
    }

    /// Lists projects with offset-based pagination
    pub async fn list_offset(
        pool: &DbPool,
        order: SortOrder,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<Project>, i64)> {
        let offset = crate::pagination::offset(page, per_page);

        let total_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects")
            .fetch_one(pool)
            .await?;

        let order_clause = match order {
            SortOrder::Asc => "ORDER BY created_at ASC",
            SortOrder::Desc => "ORDER BY created_at DESC",
        };

        let query = format!(
            r#"
            SELECT id, name, slug, sentry_key, stored_event_count,
                   digested_event_count, created_at, updated_at, platform,
                   quota_exceeded_until, quota_exceeded_reason, next_quota_check,
                   quota_minute_window, quota_minute_count, quota_hour_window, quota_hour_count,
                   rate_limited_event_count, rate_limit_per_minute, rate_limit_per_hour,
                   disabled_products, product_order
            FROM projects
            {}
            LIMIT $1 OFFSET $2
            "#,
            order_clause
        );

        let projects = sqlx::query_as::<_, Project>(sqlx::AssertSqlSafe(&*query))
            .bind(per_page)
            .bind(offset)
            .fetch_all(pool)
            .await?;

        Ok((projects, total_count.0))
    }

    /// Lists projects with offset-based pagination, restricted to the given ids.
    ///
    /// Used to scope the project list for non-admin members to only the projects
    /// they belong to. If `ids` is empty, returns `(vec![], 0)` without issuing a
    /// query (avoids an invalid `IN ()` clause).
    pub async fn list_offset_for_ids(
        pool: &DbPool,
        ids: &[i32],
        order: SortOrder,
        page: i64,
        per_page: i64,
    ) -> AppResult<(Vec<Project>, i64)> {
        if ids.is_empty() {
            return Ok((Vec::new(), 0));
        }

        let offset = crate::pagination::offset(page, per_page);

        // Build a parameterized IN list ($1, $2, ...). Bind params start at $1
        // for the ids; LIMIT/OFFSET come after.
        let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("${}", i)).collect();
        let in_clause = placeholders.join(", ");
        let limit_param = ids.len() + 1;
        let offset_param = ids.len() + 2;

        let order_clause = match order {
            SortOrder::Asc => "ORDER BY created_at ASC",
            SortOrder::Desc => "ORDER BY created_at DESC",
        };

        let count_query = format!("SELECT COUNT(*) FROM projects WHERE id IN ({})", in_clause);
        let mut count_q = sqlx::query_as::<_, (i64,)>(sqlx::AssertSqlSafe(&*count_query));
        for id in ids {
            count_q = count_q.bind(id);
        }
        let total_count: (i64,) = count_q.fetch_one(pool).await?;

        let query = format!(
            r#"
            SELECT id, name, slug, sentry_key, stored_event_count,
                   digested_event_count, created_at, updated_at, platform,
                   quota_exceeded_until, quota_exceeded_reason, next_quota_check,
                   quota_minute_window, quota_minute_count, quota_hour_window, quota_hour_count,
                   rate_limited_event_count, rate_limit_per_minute, rate_limit_per_hour,
                   disabled_products, product_order
            FROM projects
            WHERE id IN ({})
            {}
            LIMIT ${} OFFSET ${}
            "#,
            in_clause, order_clause, limit_param, offset_param
        );

        let mut q = sqlx::query_as::<_, Project>(sqlx::AssertSqlSafe(&*query));
        for id in ids {
            q = q.bind(id);
        }
        let projects = q.bind(per_page).bind(offset).fetch_all(pool).await?;

        Ok((projects, total_count.0))
    }

    pub async fn get_by_id(pool: &DbPool, id: i32) -> AppResult<Project> {
        let project = sqlx::query_as::<_, Project>(
            r#"
            SELECT id, name, slug, sentry_key, stored_event_count,
                   digested_event_count, created_at, updated_at, platform,
                   quota_exceeded_until, quota_exceeded_reason, next_quota_check,
                   quota_minute_window, quota_minute_count, quota_hour_window, quota_hour_count,
                   rate_limited_event_count, rate_limit_per_minute, rate_limit_per_hour,
                   disabled_products, product_order
            FROM projects
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Project with id {} not found", id)))?;

        Ok(project)
    }

    /// [`Self::get_by_id`] together with the installation's quota windows, in
    /// one round trip: what ingest and the digest read before an event's
    /// quota check.
    pub async fn get_with_installation_quota(
        pool: &DbPool,
        id: i32,
    ) -> AppResult<(Project, QuotaWindows)> {
        #[derive(sqlx::FromRow)]
        struct Row {
            #[sqlx(flatten)]
            project: Project,
            installation_minute_window: i64,
            installation_minute_count: i64,
            installation_hour_window: i64,
            installation_hour_count: i64,
        }
        let row = sqlx::query_as::<_, Row>(
            r#"
            SELECT p.id, p.name, p.slug, p.sentry_key, p.stored_event_count,
                   p.digested_event_count, p.created_at, p.updated_at, p.platform,
                   p.quota_exceeded_until, p.quota_exceeded_reason, p.next_quota_check,
                   p.quota_minute_window, p.quota_minute_count, p.quota_hour_window, p.quota_hour_count,
                   p.rate_limited_event_count, p.rate_limit_per_minute, p.rate_limit_per_hour,
                   p.disabled_products, p.product_order,
                   i.quota_minute_window AS installation_minute_window,
                   i.quota_minute_count AS installation_minute_count,
                   i.quota_hour_window AS installation_hour_window,
                   i.quota_hour_count AS installation_hour_count
            FROM projects p CROSS JOIN installation i
            WHERE p.id = $1 AND i.id = 1
            "#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Project with id {} not found", id)))?;

        Ok((
            row.project,
            QuotaWindows {
                quota_minute_window: row.installation_minute_window,
                quota_minute_count: row.installation_minute_count,
                quota_hour_window: row.installation_hour_window,
                quota_hour_count: row.installation_hour_count,
            },
        ))
    }

    /// The id of the project a sentry-cli style path names: by slug first,
    /// then by numeric id (sentry-cli may send either).
    pub async fn id_by_slug_or_id(pool: &DbPool, project_ref: &str) -> AppResult<Option<i32>> {
        let by_slug: Option<i32> = sqlx::query_scalar("SELECT id FROM projects WHERE slug = $1")
            .bind(project_ref)
            .fetch_optional(pool)
            .await?;
        if by_slug.is_some() {
            return Ok(by_slug);
        }
        let Ok(numeric_id) = project_ref.parse::<i32>() else {
            return Ok(None);
        };
        Ok(sqlx::query_scalar("SELECT id FROM projects WHERE id = $1")
            .bind(numeric_id)
            .fetch_optional(pool)
            .await?)
    }

    pub async fn create(pool: &DbPool, input: CreateProject) -> AppResult<Project> {
        // Validate name
        let name = input.name.trim();
        if name.is_empty() {
            return Err(AppError::Validation("Name cannot be empty".to_string()));
        }
        if name.len() > 255 {
            return Err(AppError::Validation(
                "Name cannot exceed 255 characters".to_string(),
            ));
        }

        let platform = Self::validate_platform(input.platform.as_deref())?;

        // A slug the user typed and a slug derived from the name get opposite
        // treatment on collision, for the same reason `update` returns a
        // Conflict: when the user chose the value, storing a de-duplicated
        // variant of it silently is wrong. When we derived it from the name
        // the user never chose anything, so appending `-1` is helpful.
        let (slug, slug_is_user_chosen) = match input.slug.as_deref().map(str::trim) {
            Some(typed) if !typed.is_empty() => {
                let slugified = slugify(typed);
                if slugified.is_empty() {
                    return Err(AppError::Validation(
                        "Cannot generate valid slug from input".to_string(),
                    ));
                }
                // Pre-check so the ordinary case is a clean Conflict. The
                // insert below still has to handle the TOCTOU race.
                let taken: Option<i32> =
                    sqlx::query_scalar("SELECT id FROM projects WHERE slug = $1")
                        .bind(&slugified)
                        .fetch_optional(pool)
                        .await?;
                if taken.is_some() {
                    return Err(AppError::Conflict(format!(
                        "Project with slug '{}' already exists",
                        slugified
                    ))
                    .with_field("slug", FieldErrorCode::AlreadyExists));
                }
                (slugified, true)
            }
            _ => (Self::generate_unique_slug(pool, name).await?, false),
        };

        // Generate sentry_key in application (for cross-DB compatibility)
        let sentry_key = uuid::Uuid::new_v4();

        let project = Self::try_insert_with_retry(
            pool,
            name,
            &slug,
            sentry_key,
            platform,
            slug_is_user_chosen,
        )
        .await?;

        Ok(project)
    }

    /// Validates a user-supplied platform id against [`SELECTABLE_PLATFORMS`].
    ///
    /// Shared by [`Self::create`] and [`Self::update`] so the two can never
    /// drift. Deliberately *not* used by
    /// [`Self::infer_platform_from_event`], which filters on the narrower
    /// [`VALID_PLATFORMS`]: an event's platform and a project's platform are
    /// different domains with different valid-value lists.
    fn validate_platform(platform: Option<&str>) -> AppResult<Option<&str>> {
        match platform {
            Some(p) => {
                if !SELECTABLE_PLATFORMS.contains(&p) {
                    return Err(AppError::Validation(format!(
                        "'{}' is not a valid platform",
                        p
                    )));
                }
                Ok(Some(p))
            }
            None => Ok(None),
        }
    }

    pub async fn update(pool: &DbPool, id: i32, input: UpdateProject) -> AppResult<Project> {
        Self::update_inner(pool, id, input, SlugPrecheck::Run).await
    }

    /// [`Self::update`] with the slug availability pre-check skipped, which is
    /// the only way a test can reach the unique-violation mapping below:
    /// production reaches it only by losing a TOCTOU race. Mirrors
    /// [`Self::create_with_stale_slug`] on the update path.
    ///
    /// **Test seam**, and not merely `#[doc(hidden)]`: hiding it from rustdoc
    /// left it in the binary and callable, and this one reads like a
    /// legitimate variant of `update` while silently skipping the slug
    /// pre-check. It does not exist in the shipped binary.
    ///
    /// `Cargo.toml` enables `test-seams` through a self dev-dependency, which
    /// reaches test targets and never `cargo build --release`.
    #[cfg(feature = "test-seams")]
    #[doc(hidden)]
    pub async fn update_with_raced_slug(
        pool: &DbPool,
        id: i32,
        input: UpdateProject,
    ) -> AppResult<Project> {
        Self::update_inner(pool, id, input, SlugPrecheck::Skip).await
    }

    async fn update_inner(
        pool: &DbPool,
        id: i32,
        input: UpdateProject,
        slug_precheck: SlugPrecheck,
    ) -> AppResult<Project> {
        Self::get_by_id(pool, id).await?;

        if input
            .disabled_products
            .as_ref()
            .is_some_and(|off| off.contains(&Product::Issues))
        {
            return Err(AppError::Validation(
                "issues cannot be disabled: a project always keeps its errors".to_string(),
            ));
        }

        let name = match input.name {
            Some(ref name) => {
                let name = name.trim();
                if name.is_empty() {
                    return Err(AppError::Validation("Name cannot be empty".to_string()));
                }
                if name.len() > 255 {
                    return Err(AppError::Validation(
                        "Name cannot exceed 255 characters".to_string(),
                    ));
                }
                Some(name)
            }
            None => None,
        };

        let platform = Self::validate_platform(input.platform.as_deref())?;
        for (field, limit) in [
            ("rate_limit_per_minute", input.rate_limit_per_minute),
            ("rate_limit_per_hour", input.rate_limit_per_hour),
        ] {
            if matches!(limit, Some(Some(value)) if value < 1) {
                return Err(AppError::Validation(format!("{field} must be at least 1"))
                    .with_field(field, FieldErrorCode::Invalid));
            }
        }

        let slug = match input.slug {
            Some(ref slug) => {
                let slugified = slugify(slug.trim());
                if slugified.is_empty() {
                    return Err(AppError::Validation(
                        "Cannot generate valid slug from input".to_string(),
                    ));
                }
                // Explicit availability check so the normal path returns a
                // clear Conflict rather than a raw unique violation. This is
                // TOCTOU-racy on its own, so the unique-violation mapping
                // below still has to cover the slug case.
                let taken: Option<i32> = match slug_precheck {
                    SlugPrecheck::Run => {
                        sqlx::query_scalar("SELECT id FROM projects WHERE slug = $1 AND id != $2")
                            .bind(&slugified)
                            .bind(id)
                            .fetch_optional(pool)
                            .await?
                    }
                    SlugPrecheck::Skip => None,
                };
                if taken.is_some() {
                    return Err(AppError::Conflict(format!(
                        "Project with slug '{}' already exists",
                        slugified
                    ))
                    .with_field("slug", FieldErrorCode::AlreadyExists));
                }
                Some(slugified)
            }
            None => None,
        };

        if name.is_none()
            && platform.is_none()
            && slug.is_none()
            && input.rate_limit_per_minute.is_none()
            && input.rate_limit_per_hour.is_none()
            && input.disabled_products.is_none()
            && input.product_order.is_none()
        {
            // If no fields to update, return project unchanged
            return Self::get_by_id(pool, id).await;
        }

        // COALESCE keeps whichever field wasn't provided at its current value,
        // so a single query handles name-only, platform-only, or both together.
        let project = sqlx::query_as::<_, Project>(
            r#"
            UPDATE projects
            SET name = COALESCE($1, name),
                platform = COALESCE($2, platform),
                slug = COALESCE($3, slug),
                rate_limit_per_minute = CASE WHEN $5 THEN $6 ELSE rate_limit_per_minute END,
                rate_limit_per_hour = CASE WHEN $7 THEN $8 ELSE rate_limit_per_hour END,
                disabled_products = CASE WHEN $9 THEN $10 ELSE disabled_products END,
                product_order = CASE WHEN $11 THEN $12 ELSE product_order END,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = $4
            RETURNING id, name, slug, sentry_key, stored_event_count,
                      digested_event_count, created_at, updated_at, platform,
                      quota_exceeded_until, quota_exceeded_reason, next_quota_check,
                   quota_minute_window, quota_minute_count, quota_hour_window, quota_hour_count,
                   rate_limited_event_count, rate_limit_per_minute, rate_limit_per_hour,
                   disabled_products, product_order
            "#,
        )
        .bind(name)
        .bind(platform)
        .bind(slug.as_deref())
        .bind(id)
        .bind(input.rate_limit_per_minute.is_some())
        .bind(input.rate_limit_per_minute.flatten())
        .bind(input.rate_limit_per_hour.is_some())
        .bind(input.rate_limit_per_hour.flatten())
        .bind(input.disabled_products.is_some())
        .bind(sqlx::types::Json(
            input.disabled_products.unwrap_or_default(),
        ))
        .bind(input.product_order.is_some())
        .bind(sqlx::types::Json(input.product_order.unwrap_or_default()))
        .fetch_one(pool)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.is_unique_violation() {
                    // The availability check above is TOCTOU-racy: a concurrent
                    // update can take the slug between the SELECT and this
                    // UPDATE. Without this arm a slug-only update loses the
                    // race as a 500 instead of a 409. Both dialects name the
                    // column in the message (Postgres `projects_slug_key`,
                    // SQLite `projects.slug`), which is what distinguishes it
                    // from the name constraint.
                    if db_err.message().contains("slug") {
                        if let Some(ref slug) = slug {
                            return AppError::Conflict(format!(
                                "Project with slug '{}' already exists",
                                slug
                            ))
                            .with_field("slug", FieldErrorCode::AlreadyExists);
                        }
                    }
                    if let Some(name) = name {
                        return AppError::Conflict(format!(
                            "Project with name '{}' already exists",
                            name
                        ))
                        .with_field("name", FieldErrorCode::AlreadyExists);
                    }
                }
            }
            AppError::Database(e)
        })?;

        Ok(project)
    }

    /// Auto-detects the project's platform from an ingested event, mirroring
    /// Sentry's `_set_project_platform_if_needed` (event_manager.py): only
    /// writes if the project has no platform yet, and only if
    /// `event_platform` is one of Relay's [`VALID_PLATFORMS`]. No-op
    /// otherwise. Never overwrites a platform once set.
    pub async fn infer_platform_from_event(
        pool: &DbPool,
        project_id: i32,
        event_platform: &str,
    ) -> AppResult<()> {
        if !VALID_PLATFORMS.contains(&event_platform) {
            return Ok(());
        }

        sqlx::query("UPDATE projects SET platform = $1 WHERE id = $2 AND platform IS NULL")
            .bind(event_platform)
            .bind(project_id)
            .execute(pool)
            .await?;

        Ok(())
    }

    /// Deletes a project (hard delete)
    pub async fn delete(pool: &DbPool, id: i32) -> AppResult<()> {
        let result = sqlx::query("DELETE FROM projects WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;

        if result.rows_affected() == 0 {
            return Err(AppError::NotFound(format!(
                "Project with id {} not found",
                id
            )));
        }

        Ok(())
    }

    /// Generates a unique slug based on the name.
    ///
    /// De-duplication is only ever correct for a *derived* slug. A slug the
    /// user typed is handled in [`Self::create`] and conflicts instead.
    async fn generate_unique_slug(pool: &DbPool, name: &str) -> AppResult<String> {
        let base_slug = slugify(name);

        if base_slug.is_empty() {
            return Err(AppError::Validation(
                "Cannot generate valid slug from name".to_string(),
            ));
        }

        // Find similar slugs
        let similar_slugs: Vec<String> =
            sqlx::query_scalar("SELECT slug FROM projects WHERE slug LIKE $1 || '%'")
                .bind(&base_slug)
                .fetch_all(pool)
                .await?;

        if !similar_slugs.contains(&base_slug) {
            return Ok(base_slug);
        }

        // Find the next available number
        let mut counter = 1;
        loop {
            let candidate = format!("{}-{}", base_slug, counter);
            if !similar_slugs.contains(&candidate) {
                return Ok(candidate);
            }
            counter += 1;
            if counter > 1000 {
                return Err(AppError::Internal(
                    "Could not generate unique slug".to_string(),
                ));
            }
        }
    }

    /// Bypass slug generation and directly try INSERT with a pre-computed
    /// (possibly stale) slug. Used in integration tests to simulate TOCTOU.
    ///
    /// **Test seam**, gated like [`Self::update_with_raced_slug`]: it is absent
    /// from the shipped binary.
    #[cfg(feature = "test-seams")]
    #[doc(hidden)]
    pub async fn create_with_stale_slug(
        pool: &DbPool,
        name: &str,
        stale_slug: &str,
        platform: Option<&str>,
    ) -> AppResult<Project> {
        let sentry_key = uuid::Uuid::new_v4();
        // `false`: this helper exists to exercise the derived-slug retry path.
        Self::try_insert_with_retry(pool, name, stale_slug, sentry_key, platform, false).await
    }

    /// The user-chosen half of [`Self::create_with_stale_slug`]: skips the
    /// availability pre-check in [`Self::create`] and goes straight to the
    /// INSERT with a slug the caller typed.
    ///
    /// Exists because the branch it reaches is otherwise reachable only by
    /// losing a real TOCTOU race, which no test can lose on purpose.
    ///
    /// **Test seam**, gated like [`Self::update_with_raced_slug`]: it is absent
    /// from the shipped binary.
    #[cfg(feature = "test-seams")]
    #[doc(hidden)]
    pub async fn create_with_raced_user_slug(
        pool: &DbPool,
        name: &str,
        slug: &str,
        platform: Option<&str>,
    ) -> AppResult<Project> {
        let sentry_key = uuid::Uuid::new_v4();
        Self::try_insert_with_retry(pool, name, slug, sentry_key, platform, true).await
    }

    async fn try_insert_with_retry(
        pool: &DbPool,
        name: &str,
        slug: &str,
        sentry_key: uuid::Uuid,
        platform: Option<&str>,
        slug_is_user_chosen: bool,
    ) -> AppResult<Project> {
        const INSERT_SQL: &str = r#"
            INSERT INTO projects (name, slug, sentry_key, platform)
            VALUES ($1, $2, $3, $4)
            RETURNING id, name, slug, sentry_key, stored_event_count,
                      digested_event_count, created_at, updated_at, platform,
                      quota_exceeded_until, quota_exceeded_reason, next_quota_check,
                   quota_minute_window, quota_minute_count, quota_hour_window, quota_hour_count,
                   rate_limited_event_count, rate_limit_per_minute, rate_limit_per_hour,
                   disabled_products, product_order
        "#;

        let result = sqlx::query_as::<_, Project>(INSERT_SQL)
            .bind(name)
            .bind(slug)
            .bind(sentry_key)
            .bind(platform)
            .fetch_one(pool)
            .await;

        match result {
            Ok(p) => Ok(p),
            Err(sqlx::Error::Database(ref db_err)) if db_err.is_unique_violation() => {
                if slug_is_user_chosen {
                    // Lost the TOCTOU race on a slug the user typed. Retrying
                    // under a renamed slug would store something they never
                    // asked for, so report whichever column actually collided.
                    let slug_taken: Option<i32> =
                        sqlx::query_scalar("SELECT id FROM projects WHERE slug = $1")
                            .bind(slug)
                            .fetch_optional(pool)
                            .await?;
                    // The field annotation belongs inside the branch, not on
                    // the `return`: the two arms blame two different inputs,
                    // so one annotation for both would mislabel one of them.
                    return Err(if slug_taken.is_some() {
                        AppError::Conflict(format!("Project with slug '{}' already exists", slug))
                            .with_field("slug", FieldErrorCode::AlreadyExists)
                    } else {
                        AppError::Conflict(format!("Project with name '{}' already exists", name))
                            .with_field("name", FieldErrorCode::AlreadyExists)
                    });
                }

                // The slug may have been taken by a concurrent request (TOCTOU).
                // Re-query to get the next available slug and retry once.
                let new_slug = Self::generate_unique_slug(pool, name).await?;
                if new_slug == slug {
                    // Slug is still available → the collision was on name, not slug.
                    return Err(AppError::Conflict(format!(
                        "Project with name '{}' already exists",
                        name
                    ))
                    .with_field("name", FieldErrorCode::AlreadyExists));
                }
                sqlx::query_as::<_, Project>(INSERT_SQL)
                    .bind(name)
                    .bind(&new_slug)
                    .bind(sentry_key)
                    .bind(platform)
                    .fetch_one(pool)
                    .await
                    .map_err(|e| {
                        if let sqlx::Error::Database(ref db_err) = e {
                            if db_err.is_unique_violation() {
                                return AppError::Conflict(format!(
                                    "Project with name '{}' already exists",
                                    name
                                ))
                                .with_field("name", FieldErrorCode::AlreadyExists);
                            }
                        }
                        AppError::Database(e)
                    })
            }
            Err(e) => Err(AppError::Database(e)),
        }
    }
}
