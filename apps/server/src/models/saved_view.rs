//! Saved views (#531): a dashboard page's address saved under a name, the
//! way Sentry's Issue Views save a query, sort and filters.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use crate::error::{AppError, AppResult, FieldError, FieldErrorCode};

const MAX_PAGE: usize = 64;
const MAX_NAME: usize = 100;
const MAX_QUERY: usize = 4096;

/// Who sees a view: its creator only, or everyone who sees the project.
/// Sentry's `owner` / `organization`, with the project as the organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum ViewVisibility {
    #[default]
    Owner,
    Project,
}

impl ViewVisibility {
    pub fn as_str(self) -> &'static str {
        match self {
            ViewVisibility::Owner => "owner",
            ViewVisibility::Project => "project",
        }
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SavedView {
    pub id: i32,
    /// `null` for a workspace view, on a page that spans every project.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub project_id: Option<i32>,
    /// Who saved it; `null` once that account is gone.
    #[cfg_attr(feature = "openapi", schema(required))]
    pub created_by: Option<i32>,
    /// The dashboard page it belongs to (`issues`, `logs`, …).
    pub page: String,
    pub name: String,
    /// The page's search string, without the leading `?`.
    pub query: String,
    #[cfg_attr(feature = "openapi", schema(value_type = ViewVisibility))]
    pub visibility: String,
    /// The page opens on it for whoever has not picked their own default.
    pub is_project_default: bool,
    /// The caller picked it as their default for the page; wins over the
    /// project's.
    #[sqlx(default)]
    pub is_my_default: bool,
    pub created_at: DateTime<Utc>,
}

/// Body of `POST /api/projects/{project_id}/views`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateSavedView {
    /// The dashboard page it belongs to (`issues`, `logs`, …).
    pub page: String,
    pub name: String,
    /// The page's search string, without the leading `?`.
    pub query: String,
    /// `owner` (default) or `project`.
    #[serde(default)]
    #[schemars(with = "Option<String>")]
    pub visibility: ViewVisibility,
}

impl CreateSavedView {
    /// Trims the name and rejects what the dashboard could not show back.
    pub fn validate(mut self) -> AppResult<Self> {
        self.name = self.name.trim().to_string();
        let mut fields = Vec::new();
        for (field, value, max) in [
            ("page", &self.page, MAX_PAGE),
            ("name", &self.name, MAX_NAME),
            ("query", &self.query, MAX_QUERY),
        ] {
            if value.is_empty() && field != "query" {
                fields.push(FieldError::new(field, FieldErrorCode::Required));
            } else if value.chars().count() > max {
                fields.push(FieldError::new(field, FieldErrorCode::TooLong));
            }
        }
        if fields.is_empty() {
            Ok(self)
        } else {
            Err(AppError::Validation("Invalid view".to_string()).with_fields(fields))
        }
    }
}

/// Body of `PATCH /api/projects/{project_id}/views/{view_id}`: only the
/// fields sent change.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct UpdateSavedView {
    pub name: Option<String>,
    /// The page's search string, without the leading `?`.
    pub query: Option<String>,
    #[schemars(with = "Option<String>")]
    pub visibility: Option<ViewVisibility>,
}

impl SavedView {
    pub fn is_shared(&self) -> bool {
        self.visibility == ViewVisibility::Project.as_str()
    }

    /// This view with `changes` applied, validated as a new one would be.
    pub fn merge(&self, changes: UpdateSavedView) -> AppResult<CreateSavedView> {
        CreateSavedView {
            page: self.page.clone(),
            name: changes.name.unwrap_or_else(|| self.name.clone()),
            query: changes.query.unwrap_or_else(|| self.query.clone()),
            visibility: changes.visibility.unwrap_or(if self.is_shared() {
                ViewVisibility::Project
            } else {
                ViewVisibility::Owner
            }),
        }
        .validate()
    }
}

/// Whose default a [`SetDefaultView`] sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum DefaultScope {
    /// The caller's own, seen by them only.
    User,
    /// Everyone's who has not picked their own. Needs `project:update`.
    Project,
}

/// Body of `PUT /api/projects/{project_id}/views/default`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SetDefaultView {
    /// The dashboard page (`issues`, `logs`, …).
    pub page: String,
    /// The view to open on; `null` goes back to the page's own default.
    pub view_id: Option<i32>,
    pub scope: DefaultScope,
}

/// Query of `GET /api/projects/{project_id}/views`.
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListSavedViews {
    /// Only this page's views.
    pub page: String,
}
