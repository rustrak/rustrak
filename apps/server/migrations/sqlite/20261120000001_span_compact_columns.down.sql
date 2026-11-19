UPDATE spans SET parent_span_id = printf('%016x', parent_span_id_int) WHERE parent_span_id_int IS NOT NULL;
UPDATE spans SET segment_id = printf('%016x', segment_id_int) WHERE segment_id_int IS NOT NULL;
UPDATE spans SET
    platform = COALESCE(platform, (SELECT NULLIF(t.platform, '') FROM transactions t WHERE t.id = spans.transaction_id)),
    release = COALESCE(release, (SELECT NULLIF(t.release, '') FROM transactions t WHERE t.id = spans.transaction_id)),
    environment = COALESCE(environment, (SELECT NULLIF(t.environment, '') FROM transactions t WHERE t.id = spans.transaction_id))
WHERE transaction_id IS NOT NULL;
ALTER TABLE spans DROP COLUMN parent_span_id_int;
ALTER TABLE spans DROP COLUMN segment_id_int;
