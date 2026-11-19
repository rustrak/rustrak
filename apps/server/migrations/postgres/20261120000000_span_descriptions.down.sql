UPDATE spans SET description = d.text
FROM span_descriptions d WHERE d.id = spans.description_id;
ALTER TABLE spans DROP COLUMN description_id;
DROP TABLE span_descriptions;
