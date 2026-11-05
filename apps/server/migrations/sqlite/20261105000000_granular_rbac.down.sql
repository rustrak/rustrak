-- Hashed tokens cannot be turned back into plaintext: revoke them all.
DELETE FROM auth_tokens WHERE length(token) = 64;
DROP TABLE auth_token_projects;
DROP TABLE auth_token_permissions;
ALTER TABLE auth_tokens DROP COLUMN expires_at;
ALTER TABLE auth_tokens DROP COLUMN all_projects;
ALTER TABLE auth_tokens DROP COLUMN all_permissions;
ALTER TABLE auth_tokens DROP COLUMN token_prefix;

ALTER TABLE invitations DROP COLUMN project_id;
ALTER TABLE invitations DROP COLUMN role_id;
CREATE TABLE project_members (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'viewer',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (project_id, user_id)
);
CREATE INDEX idx_project_members_user ON project_members(user_id);
CREATE INDEX idx_project_members_project ON project_members(project_id);
DROP TABLE role_assignments;
DROP TABLE role_permissions;
DROP TABLE roles;
