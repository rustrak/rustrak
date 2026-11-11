PRAGMA defer_foreign_keys = ON;

ALTER TABLE assembly_jobs ADD COLUMN chunks TEXT NOT NULL DEFAULT '[]';
ALTER TABLE assembly_jobs ADD COLUMN locked_until TEXT;
ALTER TABLE assembly_jobs ADD COLUMN worker_id TEXT;

UPDATE assembly_jobs SET chunks = (
    SELECT json_group_array(checksum) FROM (
        SELECT checksum FROM assembly_job_chunks r
        WHERE r.job_id = assembly_jobs.id ORDER BY position
    )
);

CREATE TABLE assembly_job_chunks_old (
    job_id   INTEGER NOT NULL REFERENCES assembly_jobs(id) ON DELETE CASCADE,
    checksum TEXT NOT NULL REFERENCES chunk(checksum) ON DELETE RESTRICT,
    PRIMARY KEY (job_id, checksum)
);
INSERT OR IGNORE INTO assembly_job_chunks_old(job_id, checksum)
SELECT job_id, checksum FROM assembly_job_chunks;
DROP TABLE assembly_job_chunks;
ALTER TABLE assembly_job_chunks_old RENAME TO assembly_job_chunks;
CREATE INDEX idx_assembly_job_chunks_checksum ON assembly_job_chunks(checksum);

UPDATE source_file SET id = lower(
    substr(hex(id), 1, 8) || '-' || substr(hex(id), 9, 4) || '-' || substr(hex(id), 13, 4)
    || '-' || substr(hex(id), 17, 4) || '-' || substr(hex(id), 21)
) WHERE typeof(id) = 'blob';
UPDATE source_file_metadata SET id = lower(
    substr(hex(id), 1, 8) || '-' || substr(hex(id), 9, 4) || '-' || substr(hex(id), 13, 4)
    || '-' || substr(hex(id), 17, 4) || '-' || substr(hex(id), 21)
) WHERE typeof(id) = 'blob';
UPDATE source_file_metadata SET debug_id = lower(
    substr(hex(debug_id), 1, 8) || '-' || substr(hex(debug_id), 9, 4) || '-'
    || substr(hex(debug_id), 13, 4) || '-' || substr(hex(debug_id), 17, 4) || '-'
    || substr(hex(debug_id), 21)
) WHERE typeof(debug_id) = 'blob';
UPDATE source_file_metadata SET file_id = lower(
    substr(hex(file_id), 1, 8) || '-' || substr(hex(file_id), 9, 4) || '-'
    || substr(hex(file_id), 13, 4) || '-' || substr(hex(file_id), 17, 4) || '-'
    || substr(hex(file_id), 21)
) WHERE typeof(file_id) = 'blob';
