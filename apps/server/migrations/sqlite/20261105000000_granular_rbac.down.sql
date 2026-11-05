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
-- Put back what the old levels can say. The level comes from what the role holds
-- now, since seeded roles are editable and a name proves nothing, and the highest
-- wins; a role on every project becomes a membership of each one that exists; a
-- role with no project permission is no membership. Anything finer than the three
-- levels (and any workspace permission) is lost with these tables.
INSERT INTO project_members (project_id, user_id, role)
SELECT p.id, ra.user_id,
    CASE MAX(CASE
        WHEN rp.resource IN ('project-members', 'client-keys') AND rp.action = 'update' THEN 3
        WHEN rp.action <> 'read' THEN 2
        ELSE 1
    END) WHEN 3 THEN 'admin' WHEN 2 THEN 'editor' ELSE 'viewer' END
FROM role_assignments ra
JOIN role_permissions rp ON rp.role_id = ra.role_id
JOIN projects p ON p.id = ra.project_id OR ra.project_id IS NULL
WHERE rp.resource IN (
    'issues', 'performance', 'logs', 'sessions', 'releases', 'source-maps', 'alert-rules',
    'monitors', 'project', 'client-keys', 'project-members', 'project-github'
)
GROUP BY p.id, ra.user_id;
DROP TABLE role_assignments;
DROP TABLE role_permissions;
DROP TABLE roles;
