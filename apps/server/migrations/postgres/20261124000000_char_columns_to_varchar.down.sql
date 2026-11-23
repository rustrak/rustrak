ALTER TABLE assembly_job_chunks ALTER COLUMN checksum TYPE CHAR(40);
ALTER TABLE chunk ALTER COLUMN checksum TYPE CHAR(40);
ALTER TABLE assembly_jobs ALTER COLUMN bundle_checksum TYPE CHAR(40);
ALTER TABLE source_file ALTER COLUMN checksum TYPE CHAR(40);
ALTER TABLE groupings ALTER COLUMN grouping_key_hash TYPE CHAR(64);
ALTER TABLE invitations ALTER COLUMN token TYPE CHAR(40);
ALTER TABLE oauth_codes ALTER COLUMN code_hash TYPE CHAR(64);
ALTER TABLE oauth_grants ALTER COLUMN access_hash TYPE CHAR(64),
                         ALTER COLUMN refresh_hash TYPE CHAR(64);
ALTER TABLE oauth_spent_refresh_tokens ALTER COLUMN refresh_hash TYPE CHAR(64);
ALTER TABLE sessions ALTER COLUMN id_hash TYPE CHAR(64);
