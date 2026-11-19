UPDATE spans SET description = (SELECT text FROM span_descriptions d WHERE d.id = spans.description_id)
WHERE description_id IS NOT NULL;
ALTER TABLE spans DROP COLUMN description_id;
DROP TABLE span_descriptions;
