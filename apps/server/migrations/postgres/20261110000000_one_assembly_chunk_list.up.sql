-- One chunk list per job, ordered, in assembly_job_chunks; the lease
-- columns go: one process assembles, one job at a time.

-- A bundle may repeat a chunk, so a job's chunks are keyed by position.
CREATE TABLE assembly_job_chunks_new (
    job_id   BIGINT NOT NULL REFERENCES assembly_jobs(id) ON DELETE CASCADE,
    position INT NOT NULL,
    checksum CHAR(40) NOT NULL REFERENCES chunk(checksum) ON DELETE RESTRICT,
    PRIMARY KEY (job_id, position)
);

INSERT INTO assembly_job_chunks_new(job_id, position, checksum)
SELECT jobs.id, chunks.ordinality - 1, chunks.checksum
FROM assembly_jobs jobs
CROSS JOIN LATERAL unnest(jobs.chunks) WITH ORDINALITY AS chunks(checksum, ordinality)
JOIN chunk ON chunk.checksum = chunks.checksum
WHERE jobs.id IN (SELECT job_id FROM assembly_job_chunks);

DROP TABLE assembly_job_chunks;
ALTER TABLE assembly_job_chunks_new RENAME TO assembly_job_chunks;
ALTER TABLE assembly_job_chunks RENAME CONSTRAINT assembly_job_chunks_new_pkey TO assembly_job_chunks_pkey;
CREATE INDEX idx_assembly_job_chunks_checksum ON assembly_job_chunks(checksum);

ALTER TABLE assembly_jobs
    DROP COLUMN chunks,
    DROP COLUMN locked_until,
    DROP COLUMN worker_id;
