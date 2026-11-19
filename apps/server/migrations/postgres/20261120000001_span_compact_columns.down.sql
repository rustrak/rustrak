UPDATE spans SET parent_span_id = lpad(to_hex(parent_span_id_int), 16, '0') WHERE parent_span_id_int IS NOT NULL;
UPDATE spans SET segment_id = lpad(to_hex(segment_id_int), 16, '0') WHERE segment_id_int IS NOT NULL;
UPDATE spans s SET platform = COALESCE(s.platform, NULLIF(t.platform, '')),
                   release = COALESCE(s.release, NULLIF(t.release, '')),
                   environment = COALESCE(s.environment, NULLIF(t.environment, ''))
FROM transactions t WHERE t.id = s.transaction_id;
ALTER TABLE spans DROP COLUMN parent_span_id_int;
ALTER TABLE spans DROP COLUMN segment_id_int;
