-- Indexes that duplicate the one a UNIQUE constraint already keeps on the
-- same column, or that are a prefix of a unique index: every write paid
-- for them and no plan needed them. Small tables, so a plain DROP.
DROP INDEX IF EXISTS idx_projects_slug;
DROP INDEX IF EXISTS idx_projects_sentry_key;
DROP INDEX IF EXISTS idx_users_email;
DROP INDEX IF EXISTS idx_auth_tokens_token;
-- UNIQUE (project_id, debug_id, file_type) serves (project_id, debug_id).
DROP INDEX IF EXISTS idx_sfm_lookup;
