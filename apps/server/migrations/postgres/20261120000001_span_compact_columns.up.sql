-- A span's parent and segment ids as the 8 bytes they are (issue #558).
-- Sixteen hex characters take 17 bytes as text; a span id is a 64-bit
-- number, which is how Sentry's own storage keeps it. New rows write the
-- number here and leave the text column NULL; an id that is not 16
-- lowercase hex characters stays as text, so every id reads back as sent.
--
-- These two are only ever read. `trace_id` and `span_id` stay as they are:
-- they are in the indexes every trace lookup uses.
--
-- Metadata only: nullable columns with no default.
ALTER TABLE spans ADD COLUMN parent_span_id_int BIGINT;
ALTER TABLE spans ADD COLUMN segment_id_int BIGINT;
