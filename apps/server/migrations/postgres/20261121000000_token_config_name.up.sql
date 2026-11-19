-- A token declared in rustrak.toml (issue #41), by the name it has there.
-- NULL for every other token. Removing the name from the file revokes the
-- token; a restart never touches a token whose config_name is NULL.
ALTER TABLE auth_tokens ADD COLUMN config_name VARCHAR(255);
CREATE UNIQUE INDEX idx_auth_tokens_config_name ON auth_tokens(config_name);
