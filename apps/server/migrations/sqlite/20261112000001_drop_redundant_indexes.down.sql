CREATE INDEX IF NOT EXISTS idx_sfm_lookup ON source_file_metadata(project_id, debug_id);
CREATE INDEX IF NOT EXISTS idx_auth_tokens_token ON auth_tokens(token);
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_projects_sentry_key ON projects(sentry_key);
CREATE INDEX IF NOT EXISTS idx_projects_slug ON projects(slug);
