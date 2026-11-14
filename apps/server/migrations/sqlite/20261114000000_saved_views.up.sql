-- Saved views (#531): a page's address (its search string) under a name.
-- `project_id` NULL is a workspace view, for a page that spans every project.
-- `visibility` 'owner' is seen by its creator only, 'project' by everyone
-- who sees the project (or the workspace). The server never reads `query`:
-- a new page needs no migration.
CREATE TABLE saved_views (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER REFERENCES projects(id) ON DELETE CASCADE,
    created_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
    page TEXT NOT NULL,
    name TEXT NOT NULL,
    query TEXT NOT NULL,
    visibility TEXT NOT NULL CHECK (visibility IN ('owner', 'project')),
    -- The view this page opens on for whoever has not picked their own.
    is_project_default BOOLEAN NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_saved_views_project_page ON saved_views(project_id, page);

-- The view each person opens a page on, which wins over the project's.
-- One per person, project (or workspace) and page: setting one replaces the
-- others of that page, in the same transaction.
CREATE TABLE saved_view_defaults (
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    view_id INTEGER NOT NULL REFERENCES saved_views(id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, view_id)
);

CREATE INDEX idx_saved_view_defaults_view ON saved_view_defaults(view_id);
