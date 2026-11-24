use serde::{Deserialize, Serialize};

/// What the sidebar sorts the projects by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum SidebarSort {
    /// The reader's own order, `order`.
    #[default]
    Manual,
    Name,
    /// The project with the most recent issue first.
    Activity,
}

impl SidebarSort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Name => "name",
            Self::Activity => "activity",
        }
    }

    /// A value a later version stopped knowing reads as the default.
    pub fn parse(value: &str) -> Self {
        match value {
            "name" => Self::Name,
            "activity" => Self::Activity,
            _ => Self::Manual,
        }
    }
}

/// How one reader's sidebar lists the projects.
///
/// Ids are kept as given: a project deleted later stays in the lists and is
/// skipped by whoever reads them, as it is no longer in the project list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SidebarProjects {
    pub sort: SidebarSort,
    /// Project ids in the reader's order; projects missing from it follow.
    pub order: Vec<i32>,
    /// Project ids the sidebar leaves out.
    pub hidden: Vec<i32>,
}
