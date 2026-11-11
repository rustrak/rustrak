-- Source map tables in the types every other table uses: UUIDs as the
-- 16-byte blobs sqlx binds (the rows were written as hyphenated text), and
-- one chunk list per job, ordered, in assembly_job_chunks. The lease columns
-- go too: one process assembles, one job at a time.

-- source_file.id is rewritten under its referencing metadata rows, so the
-- foreign keys are checked at commit, once both sides agree.
PRAGMA defer_foreign_keys = ON;

UPDATE source_file SET id = unhex(replace(id, '-', '')) WHERE typeof(id) = 'text';
UPDATE source_file_metadata SET id = unhex(replace(id, '-', '')) WHERE typeof(id) = 'text';
UPDATE source_file_metadata SET debug_id = unhex(replace(debug_id, '-', '')) WHERE typeof(debug_id) = 'text';
UPDATE source_file_metadata SET file_id = unhex(replace(file_id, '-', '')) WHERE typeof(file_id) = 'text';

-- A bundle may repeat a chunk, so a job's chunks are keyed by position.
CREATE TABLE assembly_job_chunks_new (
    job_id   INTEGER NOT NULL REFERENCES assembly_jobs(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    checksum TEXT NOT NULL REFERENCES chunk(checksum) ON DELETE RESTRICT,
    PRIMARY KEY (job_id, position)
);

INSERT INTO assembly_job_chunks_new(job_id, position, checksum)
SELECT jobs.id, CAST(json_each.key AS INTEGER), json_each.value
FROM assembly_jobs jobs, json_each(jobs.chunks)
JOIN chunk ON chunk.checksum = json_each.value
WHERE jobs.id IN (SELECT job_id FROM assembly_job_chunks);

DROP TABLE assembly_job_chunks;
ALTER TABLE assembly_job_chunks_new RENAME TO assembly_job_chunks;
CREATE INDEX idx_assembly_job_chunks_checksum ON assembly_job_chunks(checksum);

ALTER TABLE assembly_jobs DROP COLUMN chunks;
ALTER TABLE assembly_jobs DROP COLUMN locked_until;
ALTER TABLE assembly_jobs DROP COLUMN worker_id;
