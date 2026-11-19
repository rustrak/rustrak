-- A span's description stored once per project (issue #558). Measured on a
-- PHP app, the SQL text was 437 of the 818 bytes of a database span and had
-- about 200 distinct values: with the text here and the span holding only a
-- reference, the same spans took 116 MB instead of 195 MB.
--
-- `hash` is the first 16 bytes of the text's SHA-256, what a description is
-- found by. `last_used_at` is when a span was last made to refer to the row:
-- the cleanup removes a row no span refers to only once that is old.
--
-- Rows written before this keep their description in `spans.description`,
-- as do short descriptions and ones seen only once. Both statements are
-- metadata only: a new table, and a nullable column with no default.
CREATE TABLE span_descriptions (
    id           BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    project_id   INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    hash         BYTEA NOT NULL,
    text         TEXT NOT NULL,
    last_used_at TIMESTAMPTZ NOT NULL,
    UNIQUE (project_id, hash)
);

ALTER TABLE spans ADD COLUMN description_id BIGINT;
