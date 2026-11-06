-- Granular RBAC: a role is a named set of (resource, action) permissions; an
-- assignment says where a user holds it. The catalog of resources and actions
-- lives in code (src/auth/rbac.rs).

CREATE TABLE roles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    -- Set: the personal permissions of this one user, not a role of the list.
    user_id INTEGER REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX ux_roles_name ON roles(name) WHERE user_id IS NULL;
CREATE UNIQUE INDEX ux_roles_personal ON roles(user_id) WHERE user_id IS NOT NULL;

CREATE TABLE role_permissions (
    role_id INTEGER NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    resource TEXT NOT NULL,
    action TEXT NOT NULL,
    PRIMARY KEY (role_id, resource, action)
);

-- project_id NULL: the role's project permissions reach every project. Its
-- workspace permissions apply wherever it is held.
CREATE TABLE role_assignments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id INTEGER NOT NULL REFERENCES roles(id),
    project_id INTEGER REFERENCES projects(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE UNIQUE INDEX ux_role_assignments ON role_assignments(user_id, role_id, COALESCE(project_id, 0));
CREATE INDEX idx_role_assignments_project ON role_assignments(project_id);

-- A new install has no roles. Existing project memberships keep their access:
-- each level in use becomes an ordinary role, editable and deletable. Instance
-- owners are not a role: they are users.role = 'admin', and hold everything.
INSERT INTO roles (name, description)
SELECT v.column1, v.column2 FROM (VALUES
    ('Project Admin', 'Runs the project: everything Editor does, plus managing its members and deleting the project.', 'admin'),
    ('Editor', 'Does the daily work: triages, resolves and deletes issues; manages releases, source maps, alert rules and cron monitors; edits the project settings and repository link. Cannot manage members or delete the project.', 'editor'),
    ('Viewer', 'Sees everything in the project (issues, performance, logs, releases, alerts, monitors, settings) and changes nothing.', 'viewer')
) AS v
WHERE EXISTS (
    SELECT 1 FROM project_members pm
    WHERE v.column3 = CASE pm.role WHEN 'admin' THEN 'admin' WHEN 'editor' THEN 'editor' ELSE 'viewer' END
);

INSERT INTO role_permissions (role_id, resource, action)
SELECT r.id, v.column1, v.column2 FROM roles r, (VALUES
    ('issues', 'read'), ('issues', 'update'), ('issues', 'delete'),
    ('performance', 'read'), ('logs', 'read'), ('sessions', 'read'),
    ('releases', 'read'), ('releases', 'create'), ('releases', 'update'),
    ('source-maps', 'read'), ('source-maps', 'create'),
    ('alert-rules', 'read'), ('alert-rules', 'create'), ('alert-rules', 'update'), ('alert-rules', 'delete'),
    ('monitors', 'read'), ('monitors', 'create'), ('monitors', 'update'), ('monitors', 'delete'),
    ('project', 'update'), ('project', 'delete'),
    ('client-keys', 'read'),
    ('project-members', 'read'), ('project-members', 'update'),
    ('project-github', 'read'), ('project-github', 'update')
) AS v WHERE r.name = 'Project Admin';

INSERT INTO role_permissions (role_id, resource, action)
SELECT r.id, v.column1, v.column2 FROM roles r, (VALUES
    ('issues', 'read'), ('issues', 'update'), ('issues', 'delete'),
    ('performance', 'read'), ('logs', 'read'), ('sessions', 'read'),
    ('releases', 'read'), ('releases', 'create'), ('releases', 'update'),
    ('source-maps', 'read'), ('source-maps', 'create'),
    ('alert-rules', 'read'), ('alert-rules', 'create'), ('alert-rules', 'update'), ('alert-rules', 'delete'),
    ('monitors', 'read'), ('monitors', 'create'), ('monitors', 'update'), ('monitors', 'delete'),
    ('project', 'update'),
    ('client-keys', 'read'), ('project-members', 'read'),
    ('project-github', 'read'), ('project-github', 'update')
) AS v WHERE r.name = 'Editor';

INSERT INTO role_permissions (role_id, resource, action)
SELECT r.id, v.column1, 'read' FROM roles r, (VALUES
    ('issues'), ('performance'), ('logs'), ('sessions'), ('releases'), ('source-maps'),
    ('alert-rules'), ('monitors'), ('client-keys'), ('project-members'), ('project-github')
) AS v WHERE r.name = 'Viewer';

-- Anyone could create an API token before roles, limited to what they hold.
INSERT INTO role_permissions (role_id, resource, action)
SELECT id, 'api-tokens', 'create' FROM roles;

-- Carry over existing access unchanged: each project membership becomes
-- the matching project role, and project_members goes away.

INSERT INTO role_assignments (user_id, role_id, project_id)
SELECT pm.user_id, r.id, pm.project_id FROM project_members pm, roles r
WHERE r.name = CASE pm.role WHEN 'admin' THEN 'Project Admin' WHEN 'editor' THEN 'Editor' ELSE 'Viewer' END;

DROP TABLE project_members;

-- An invitation can carry one role, with the same meaning of project_id, that
-- the new user holds once they accept.
ALTER TABLE invitations ADD COLUMN role_id INTEGER REFERENCES roles(id) ON DELETE SET NULL;
ALTER TABLE invitations ADD COLUMN project_id INTEGER REFERENCES projects(id) ON DELETE CASCADE;

-- API tokens: stored as a SHA-256 hash (existing plaintext tokens are hashed
-- at startup, since SQLite has no SHA-256), limited to chosen permissions and
-- projects, and optionally expiring. Existing tokens keep full access.
ALTER TABLE auth_tokens ADD COLUMN token_prefix TEXT NOT NULL DEFAULT '';
UPDATE auth_tokens SET token_prefix = substr(token, 1, 8);
ALTER TABLE auth_tokens ADD COLUMN all_permissions BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE auth_tokens ADD COLUMN all_projects BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE auth_tokens ADD COLUMN expires_at TEXT;

CREATE TABLE auth_token_permissions (
    token_id INTEGER NOT NULL REFERENCES auth_tokens(id) ON DELETE CASCADE,
    resource TEXT NOT NULL,
    action TEXT NOT NULL,
    PRIMARY KEY (token_id, resource, action)
);

CREATE TABLE auth_token_projects (
    token_id INTEGER NOT NULL REFERENCES auth_tokens(id) ON DELETE CASCADE,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    PRIMARY KEY (token_id, project_id)
);
