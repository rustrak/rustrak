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
-- Put back what the old levels can say, without giving more than was held: a
-- level takes every permission it stands for, read from what the user's roles
-- hold now (seeded roles are editable, a name proves nothing), and a role on
-- every project counts on each one that exists. Less than a whole level is no
-- membership: that access goes with these tables, as does anything finer.
INSERT INTO project_members (project_id, user_id, role)
WITH legacy (level, resource, action) AS (VALUES
    (1, 'issues', 'read'), (1, 'performance', 'read'), (1, 'logs', 'read'), (1, 'sessions', 'read'),
    (1, 'releases', 'read'), (1, 'source-maps', 'read'), (1, 'alert-rules', 'read'), (1, 'monitors', 'read'),
    (1, 'client-keys', 'read'), (1, 'project-members', 'read'), (1, 'project-github', 'read'),
    (2, 'issues', 'update'), (2, 'issues', 'delete'),
    (2, 'releases', 'create'), (2, 'releases', 'update'), (2, 'releases', 'delete'),
    (2, 'source-maps', 'create'), (2, 'source-maps', 'delete'),
    (2, 'alert-rules', 'create'), (2, 'alert-rules', 'update'), (2, 'alert-rules', 'delete'),
    (2, 'monitors', 'create'), (2, 'monitors', 'update'), (2, 'monitors', 'delete'),
    (2, 'project', 'update'), (2, 'project-github', 'update'),
    (3, 'project', 'delete'), (3, 'client-keys', 'update'), (3, 'project-members', 'update')
), held AS (
    SELECT DISTINCT p.id AS project_id, ra.user_id, rp.resource, rp.action
    FROM role_assignments ra
    JOIN role_permissions rp ON rp.role_id = ra.role_id
    JOIN projects p ON p.id = ra.project_id OR ra.project_id IS NULL
)
SELECT project_id, user_id,
    CASE
        WHEN COUNT(*) = 29 THEN 'admin'
        WHEN SUM(CASE WHEN level <= 2 THEN 1 ELSE 0 END) = 26 THEN 'editor'
        ELSE 'viewer'
    END
FROM held JOIN legacy USING (resource, action)
GROUP BY project_id, user_id
HAVING SUM(CASE WHEN level = 1 THEN 1 ELSE 0 END) = 11;
DROP TABLE role_assignments;
DROP TABLE role_permissions;
DROP TABLE roles;
