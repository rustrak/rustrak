-- sqlx binds a Rust string as `text`. Against a CHAR(n) column Postgres casts
-- the column, not the parameter, so the column's index cannot be searched:
-- the session lookup on every request and the grouping lookup on every event
-- filtered whole index ranges (#329). VARCHAR compares to text directly.
-- The values are fixed-length hex, so nothing is padded or trimmed.
ALTER TABLE assembly_job_chunks ALTER COLUMN checksum TYPE VARCHAR(40);
ALTER TABLE chunk ALTER COLUMN checksum TYPE VARCHAR(40);
ALTER TABLE assembly_jobs ALTER COLUMN bundle_checksum TYPE VARCHAR(40);
ALTER TABLE source_file ALTER COLUMN checksum TYPE VARCHAR(40);
ALTER TABLE groupings ALTER COLUMN grouping_key_hash TYPE VARCHAR(64);
ALTER TABLE invitations ALTER COLUMN token TYPE VARCHAR(40);
ALTER TABLE oauth_codes ALTER COLUMN code_hash TYPE VARCHAR(64);
ALTER TABLE oauth_grants ALTER COLUMN access_hash TYPE VARCHAR(64),
                         ALTER COLUMN refresh_hash TYPE VARCHAR(64);
ALTER TABLE oauth_spent_refresh_tokens ALTER COLUMN refresh_hash TYPE VARCHAR(64);
ALTER TABLE sessions ALTER COLUMN id_hash TYPE VARCHAR(64);
