-- A plugin's permission is stored as `<plugin>:<resource>`, each part up to
-- 32 characters: 65 in all, which VARCHAR(50) refused. SQLite's TEXT never
-- limited it, so this migration is PostgreSQL's alone.
ALTER TABLE role_permissions ALTER COLUMN resource TYPE VARCHAR(65);
ALTER TABLE auth_token_permissions ALTER COLUMN resource TYPE VARCHAR(65);
