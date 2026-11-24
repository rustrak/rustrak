-- How one person's sidebar lists the projects: sorted by what, in which
-- manual order, and which ones it leaves out. A project not in either list
-- is shown, after the ordered ones, so a new project appears on its own.
CREATE TABLE user_sidebar_projects (
    user_id INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    sort VARCHAR(16) NOT NULL DEFAULT 'manual',
    project_order JSONB NOT NULL DEFAULT '[]',
    hidden_projects JSONB NOT NULL DEFAULT '[]'
);
