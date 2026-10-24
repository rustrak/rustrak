-- Debug information files (sentry-cli `difs/assemble`) ride the same assembly
-- queue as artifact bundles. `kind` says which assembler a job needs; `name`
-- and `debug_id` are the assemble request's, which is where a ProGuard
-- mapping's UUID comes from.
ALTER TABLE assembly_jobs ADD COLUMN kind TEXT NOT NULL DEFAULT 'artifact_bundle';
ALTER TABLE assembly_jobs ADD COLUMN name TEXT;
ALTER TABLE assembly_jobs ADD COLUMN debug_id TEXT;
