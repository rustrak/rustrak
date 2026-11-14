//! Role-based access control: the permission catalog and how a user's role
//! assignments combine into what they may do.
//!
//! The catalog lives in code (a resource, the actions it accepts, its scope
//! and its module); roles and assignments live in the database. A role is a
//! set of `(resource, action)` grants. An assignment says where a user holds
//! it: every project (`project_id = None`) or one project. Its workspace
//! permissions apply either way. Nothing is granted by default, and grants only
//! ever add up: there are no deny rules.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};

/// Whether a resource belongs to the workspace or to one project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// Instance-wide settings and administration.
    Instance,
    /// Something that belongs to one project.
    Project,
}

/// Something a grant can allow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Read,
    Create,
    Update,
    Delete,
}

impl Action {
    pub const ALL: &'static [Action] =
        &[Action::Read, Action::Create, Action::Update, Action::Delete];

    pub fn key(self) -> &'static str {
        match self {
            Action::Read => "read",
            Action::Create => "create",
            Action::Update => "update",
            Action::Delete => "delete",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|a| a.key() == s)
    }
}

use Action::{Create as C, Delete as D, Read as R, Update as U};

macro_rules! catalog {
    ($($variant:ident = $key:tt, $module:literal, $scope:ident, [$($action:ident),+];)+) => {
        /// Every resource a permission can name.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
        pub enum Resource { $(#[serde(rename = $key)] $variant),+ }

        impl Resource {
            pub const ALL: &'static [Resource] = &[$(Resource::$variant),+];

            /// Stable key, stored in the database and sent to clients.
            pub fn key(self) -> &'static str {
                match self { $(Resource::$variant => $key),+ }
            }

            /// The group the dashboard shows this resource under.
            pub fn module(self) -> &'static str {
                match self { $(Resource::$variant => $module),+ }
            }

            pub fn scope(self) -> Scope {
                match self { $(Resource::$variant => Scope::$scope),+ }
            }

            /// The actions that mean something for this resource.
            pub fn actions(self) -> &'static [Action] {
                match self { $(Resource::$variant => &[$($action),+]),+ }
            }
        }
    };
}

catalog! {
    // Instance: the workspace (projects, people, access)
    Projects = "projects", "workspace", Instance, [C];
    Users = "users", "workspace", Instance, [R, U, D];
    Invitations = "invitations", "workspace", Instance, [R, C, D];
    Roles = "roles", "workspace", Instance, [R, C, U, D];
    ApiTokens = "api-tokens", "workspace", Instance, [R, C, D];
    // Instance: settings, one resource per area
    Integrations = "integrations", "settings", Instance, [R, C, U, D];
    GithubApp = "github-app", "settings", Instance, [R, U];
    Storage = "storage", "settings", Instance, [R, U, D];
    // Reading MCP settings needs nothing: every member connects their assistant.
    Mcp = "mcp", "settings", Instance, [U];
    Telemetry = "telemetry", "settings", Instance, [R];
    // Project: issues and the data behind them
    Issues = "issues", "issues", Project, [R, U, D];
    Performance = "performance", "monitoring", Project, [R];
    Logs = "logs", "monitoring", Project, [R];
    Sessions = "sessions", "monitoring", Project, [R];
    // Project: releases
    Releases = "releases", "releases", Project, [R, C, U];
    SourceMaps = "source-maps", "releases", Project, [R, C];
    // Project: alerting
    AlertRules = "alert-rules", "alerts", Project, [R, C, U, D];
    Monitors = "monitors", "alerts", Project, [R, C, U, D];
    // Project: the project itself. Seeing it needs no permission of its own:
    // any permission on the project implies it.
    Project = "project", "project", Project, [U, D];
    ClientKeys = "client-keys", "project", Project, [R];
    ProjectMembers = "project-members", "project", Project, [R, U];
    ProjectGithub = "project-github", "project", Project, [R, U];
}

impl Resource {
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|r| r.key() == s)
    }

    pub fn accepts(self, action: Action) -> bool {
        self.actions().contains(&action)
    }
}

/// Every permission there is.
fn catalog_pairs() -> impl Iterator<Item = (Resource, Action)> {
    Resource::ALL
        .iter()
        .flat_map(|r| r.actions().iter().map(move |a| (*r, *a)))
}

/// The grants of one role (or one token).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Grants {
    all: bool,
    pairs: BTreeSet<(Resource, Action)>,
}

impl Grants {
    /// Everything, including resources added to the catalog later.
    pub fn all() -> Self {
        Grants {
            all: true,
            pairs: BTreeSet::new(),
        }
    }

    /// These pairs, minus any action the resource does not accept.
    pub fn of(pairs: impl IntoIterator<Item = (Resource, Action)>) -> Self {
        Grants {
            all: false,
            pairs: pairs.into_iter().filter(|(r, a)| r.accepts(*a)).collect(),
        }
    }

    pub fn pairs(&self) -> Vec<(Resource, Action)> {
        self.pairs.iter().copied().collect()
    }

    /// Writing a resource implies reading it: a form cannot edit what it cannot
    /// load. Not for API tokens, where reading is everyone's and creating is
    /// your own.
    fn allows(&self, resource: Resource, action: Action) -> bool {
        self.all
            || self.pairs.contains(&(resource, action))
            || (action == Action::Read
                && resource != Resource::ApiTokens
                && self.pairs.iter().any(|(r, _)| *r == resource))
    }

    fn any_project_grant(&self) -> bool {
        self.all || self.pairs.iter().any(|(r, _)| r.scope() == Scope::Project)
    }

    /// Whether both hold some project permission in common.
    fn shares_project_grant(&self, other: &Grants) -> bool {
        catalog_pairs()
            .any(|(r, a)| r.scope() == Scope::Project && self.allows(r, a) && other.allows(r, a))
    }

    fn add(&mut self, other: &Grants) {
        self.all |= other.all;
        self.pairs.extend(other.pairs.iter().copied());
    }
}

/// A role given to a user, on the instance (`None`) or on one project.
#[derive(Debug, Clone)]
pub struct Assignment {
    pub project_id: Option<i32>,
    pub grants: Grants,
}

/// Which projects someone can see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectScope {
    All,
    Only(Vec<i32>),
}

impl ProjectScope {
    /// These projects, narrowed to `requested` when a list names some: what
    /// is asked for and not reachable is left out, never an error, so a list
    /// does not tell which projects exist.
    pub fn narrowed(self, requested: Option<Vec<i32>>) -> ProjectScope {
        match (self, requested) {
            (scope, None) => scope,
            (ProjectScope::All, Some(ids)) => ProjectScope::Only(ids),
            (ProjectScope::Only(held), Some(ids)) => {
                ProjectScope::Only(ids.into_iter().filter(|id| held.contains(id)).collect())
            }
        }
    }
}

/// What an API token was limited to when it was created.
#[derive(Debug, Clone)]
pub struct TokenScope {
    pub grants: Grants,
    /// `None`: every project the owner can reach.
    pub projects: Option<Vec<i32>>,
}

impl TokenScope {
    fn allows(&self, resource: Resource, action: Action, project_id: Option<i32>) -> bool {
        if resource.scope() == Scope::Instance {
            return self.grants.allows(resource, action);
        }
        let Some(id) = project_id else { return false };
        if self.projects.as_ref().is_some_and(|ids| !ids.contains(&id)) {
            return false;
        }
        self.grants.allows(resource, action)
    }

    /// Reaches every project, so an instance-wide project grant passes.
    fn allows_everywhere(&self, resource: Resource, action: Action) -> bool {
        self.projects.is_none() && self.grants.allows(resource, action)
    }
}

/// What one user may do, resolved from all their assignments, and narrowed
/// to a token's scope when they act through one.
#[derive(Debug, Clone, Default)]
pub struct Access {
    instance: Grants,
    projects: HashMap<i32, Grants>,
    cap: Option<TokenScope>,
}

impl Access {
    pub fn new(assignments: Vec<Assignment>) -> Self {
        let mut access = Access::default();
        for a in assignments {
            match a.project_id {
                None => access.instance.add(&a.grants),
                Some(id) => access.projects.entry(id).or_default().add(&a.grants),
            }
        }
        access
    }

    /// The same access, limited to what `scope` allows: a token never does
    /// more than its owner, nor more than it was created for.
    pub fn capped(mut self, scope: TokenScope) -> Self {
        self.cap = Some(scope);
        self
    }

    fn capped_by(&self, resource: Resource, action: Action, project_id: Option<i32>) -> bool {
        self.cap
            .as_ref()
            .is_none_or(|c| c.allows(resource, action, project_id))
    }

    /// Whether `action` on `resource` is allowed. Project resources need the
    /// project; instance resources ignore it and only count instance grants.
    pub fn can(&self, resource: Resource, action: Action, project_id: Option<i32>) -> bool {
        if resource == Resource::Project && action == Action::Read {
            return project_id.is_some_and(|id| self.sees_project(id));
        }
        self.held(resource, action, project_id) && self.capped_by(resource, action, project_id)
    }

    /// Whether what a token was limited to allows this. Anyone not acting
    /// through a token passes: for what is theirs regardless of what they hold.
    pub fn scope_allows(&self, resource: Resource, action: Action) -> bool {
        self.cap
            .as_ref()
            .is_none_or(|c| c.grants.allows(resource, action))
    }

    /// These grants show a project: any permission on it does, and through a
    /// token only one the token keeps.
    fn shows_project(&self, grants: &Grants) -> bool {
        match &self.cap {
            Some(cap) => grants.shares_project_grant(&cap.grants),
            None => grants.any_project_grant(),
        }
    }

    fn sees_project(&self, id: i32) -> bool {
        let listed = self
            .cap
            .as_ref()
            .and_then(|c| c.projects.as_ref())
            .is_none_or(|ids| ids.contains(&id));
        listed
            && (self.shows_project(&self.instance)
                || self
                    .projects
                    .get(&id)
                    .is_some_and(|g| self.shows_project(g)))
    }

    fn held(&self, resource: Resource, action: Action, project_id: Option<i32>) -> bool {
        if resource.scope() == Scope::Instance {
            return self.instance.allows(resource, action);
        }
        let Some(id) = project_id else { return false };
        self.instance.allows(resource, action)
            || self
                .projects
                .get(&id)
                .is_some_and(|g| g.allows(resource, action))
    }

    /// Whether `action` on a project resource is allowed on at least one
    /// project, for what belongs to no project yet.
    pub fn can_somewhere(&self, resource: Resource, action: Action) -> bool {
        match &self.cap {
            Some(cap) if !cap.grants.allows(resource, action) => false,
            Some(TokenScope {
                projects: Some(ids),
                ..
            }) => ids.iter().any(|&id| self.held(resource, action, Some(id))),
            _ => {
                self.instance.allows(resource, action)
                    || self.projects.values().any(|g| g.allows(resource, action))
            }
        }
    }

    /// Whether `action` is allowed on a project this user just created. Their
    /// roles extend to it unless they already reach every project, and a
    /// token's list of projects does not grow to include it.
    pub fn can_on_new_project(&self, resource: Resource, action: Action, id: i32) -> bool {
        if self.cap.as_ref().is_some_and(|c| c.projects.is_some()) {
            return false;
        }
        match self.visible_projects() {
            ProjectScope::All => self.can(resource, action, Some(id)),
            ProjectScope::Only(_) => self.can_somewhere(resource, action),
        }
    }

    /// The projects this user can see at all.
    pub fn visible_projects(&self) -> ProjectScope {
        let held = if self.shows_project(&self.instance) {
            ProjectScope::All
        } else {
            let mut ids: Vec<i32> = self
                .projects
                .iter()
                .filter(|(_, g)| self.shows_project(g))
                .map(|(id, _)| *id)
                .collect();
            ids.sort_unstable();
            ProjectScope::Only(ids)
        };
        let Some(cap) = &self.cap else { return held };
        match (held, &cap.projects) {
            (held, None) => held,
            (ProjectScope::All, Some(ids)) => ProjectScope::Only(ids.clone()),
            (ProjectScope::Only(ids), Some(allowed)) => {
                ProjectScope::Only(ids.into_iter().filter(|i| allowed.contains(i)).collect())
            }
        }
    }

    /// Whether this user already holds every grant in `grants` where it would
    /// apply, so handing it out (or writing it into a role) is no escalation.
    pub fn covers(&self, grants: &Grants, project_id: Option<i32>) -> bool {
        if grants.all {
            return self.is_owner();
        }
        grants
            .pairs
            .iter()
            .all(|&(r, a)| match (r.scope(), project_id) {
                (Scope::Instance, _) => self.can(r, a, None),
                (Scope::Project, Some(p)) => self.can(r, a, Some(p)),
                // On every project, a project grant reaches every project: only an
                // instance-wide holder covers it.
                (Scope::Project, None) => {
                    self.instance.allows(r, a)
                        && self.cap.as_ref().is_none_or(|c| c.allows_everywhere(r, a))
                }
            })
    }

    /// What holds everywhere: workspace permissions, and project permissions
    /// on every project.
    pub fn instance_pairs(&self) -> Vec<(Resource, Action)> {
        catalog_pairs()
            .filter(|&(r, a)| match r.scope() {
                Scope::Instance => self.can(r, a, None),
                Scope::Project => {
                    self.instance.allows(r, a)
                        && self.cap.as_ref().is_none_or(|c| c.allows_everywhere(r, a))
                }
            })
            .collect()
    }

    /// What holds on each project named by an assignment or a token's scope.
    pub fn project_pairs(&self) -> Vec<(i32, Vec<(Resource, Action)>)> {
        let mut ids: Vec<i32> = self
            .projects
            .keys()
            .copied()
            .chain(
                self.cap
                    .iter()
                    .flat_map(|c| c.projects.iter().flatten().copied()),
            )
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids.into_iter()
            .map(|id| {
                let pairs = catalog_pairs()
                    .filter(|&(r, a)| r.scope() == Scope::Project && self.can(r, a, Some(id)))
                    .collect::<Vec<_>>();
                (id, pairs)
            })
            .filter(|(_, pairs)| !pairs.is_empty())
            .collect()
    }

    /// The projects where `action` on `resource` holds: what a list across
    /// projects may read from.
    pub fn projects_where(&self, resource: Resource, action: Action) -> ProjectScope {
        let pair = (resource, action);
        if self.instance_pairs().contains(&pair) {
            return ProjectScope::All;
        }
        ProjectScope::Only(
            self.project_pairs()
                .into_iter()
                .filter(|(_, pairs)| pairs.contains(&pair))
                .map(|(id, _)| id)
                .collect(),
        )
    }

    /// Whether this holds on the workspace or on at least one project.
    pub fn holds_somewhere(&self, resource: Resource, action: Action) -> bool {
        let pair = (resource, action);
        self.instance_pairs().contains(&pair)
            || self
                .project_pairs()
                .iter()
                .any(|(_, pairs)| pairs.contains(&pair))
    }

    /// Holds every permission on the whole instance, unrestricted.
    pub fn is_owner(&self) -> bool {
        self.instance.all
            && self
                .cap
                .as_ref()
                .is_none_or(|c| c.grants.all && c.projects.is_none())
    }
}
