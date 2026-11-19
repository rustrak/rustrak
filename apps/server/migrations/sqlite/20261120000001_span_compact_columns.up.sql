-- A span's parent and segment ids as the 8 bytes they are (issue #558).
-- See the Postgres migration of the same version.
ALTER TABLE spans ADD COLUMN parent_span_id_int INTEGER;
ALTER TABLE spans ADD COLUMN segment_id_int INTEGER;
