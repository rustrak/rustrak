-- Hashed tokens cannot be turned back into plaintext: revoke them all.
DELETE FROM auth_tokens WHERE length(token) = 64;
DROP TABLE auth_token_projects;
DROP TABLE auth_token_permissions;
ALTER TABLE auth_tokens DROP COLUMN expires_at;
ALTER TABLE auth_tokens DROP COLUMN all_projects;
ALTER TABLE auth_tokens DROP COLUMN all_permissions;
ALTER TABLE auth_tokens DROP COLUMN token_prefix;
ALTER TABLE auth_tokens ALTER COLUMN token TYPE CHAR(40);

ALTER TABLE invitations DROP COLUMN project_id;
ALTER TABLE invitations DROP COLUMN role_id;
CREATE TABLE project_members (
    id SERIAL PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL DEFAULT 'viewer',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (project_id, user_id)
);
CREATE INDEX idx_project_members_user ON project_members(user_id);
CREATE INDEX idx_project_members_project ON project_members(project_id);
DROP TABLE role_assignments;
DROP TABLE role_permissions;
DROP TABLE roles;
