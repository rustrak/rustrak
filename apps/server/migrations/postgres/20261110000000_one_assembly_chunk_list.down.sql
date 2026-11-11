ALTER TABLE assembly_jobs
    ADD COLUMN chunks TEXT[] NOT NULL DEFAULT '{}',
    ADD COLUMN locked_until TIMESTAMPTZ,
    ADD COLUMN worker_id TEXT;

UPDATE assembly_jobs SET chunks = COALESCE((
    SELECT array_agg(checksum ORDER BY position) FROM assembly_job_chunks r
    WHERE r.job_id = assembly_jobs.id
), '{}');

CREATE TABLE assembly_job_chunks_old (
    job_id   BIGINT NOT NULL REFERENCES assembly_jobs(id) ON DELETE CASCADE,
    checksum CHAR(40) NOT NULL REFERENCES chunk(checksum) ON DELETE RESTRICT,
    PRIMARY KEY (job_id, checksum)
);
INSERT INTO assembly_job_chunks_old(job_id, checksum)
SELECT DISTINCT job_id, checksum FROM assembly_job_chunks;
DROP TABLE assembly_job_chunks;
ALTER TABLE assembly_job_chunks_old RENAME TO assembly_job_chunks;
ALTER TABLE assembly_job_chunks RENAME CONSTRAINT assembly_job_chunks_old_pkey TO assembly_job_chunks_pkey;
CREATE INDEX idx_assembly_job_chunks_checksum ON assembly_job_chunks(checksum);
