-- Exact selectors use string identifiers only. Partial byte-length guards keep
-- unrelated oversized SDK values ingestible and out of the lookup indexes.
-- SQLite holds its migration write transaction throughout the index/backfill work.
CREATE INDEX idx_events_project_user_identity
    ON events (project_id, json_extract(data, '$.user.id'), timestamp DESC, id DESC)
    WHERE json_type(data, '$.user.id') = 'text'
      AND length(CAST(json_extract(data, '$.user.id') AS BLOB)) BETWEEN 1 AND 200;

-- Array tags cannot use a SQLite expression index containing json_each.
-- Keep the raw payload intact and materialize each distinct eligible request tag.
CREATE TABLE event_request_identities (
    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE ON UPDATE CASCADE,
    request_id TEXT NOT NULL CHECK (length(CAST(request_id AS BLOB)) BETWEEN 1 AND 200),
    project_id INTEGER NOT NULL,
    timestamp TEXT NOT NULL,
    PRIMARY KEY (event_id, request_id)
);
CREATE INDEX idx_events_project_request_identity
    ON event_request_identities (project_id, request_id, timestamp DESC, event_id DESC);

-- One extraction definition serves historical backfill and future writes.
-- CASE protects JSON functions from malformed/scalar entries in tag arrays.
CREATE VIEW event_request_identity_values AS
SELECT DISTINCT event_id, project_id, timestamp, request_id
FROM (
    SELECT e.id AS event_id, e.project_id, e.timestamp,
        CASE tag.type
            WHEN 'array' THEN CASE
                WHEN json_array_length(tag.value) = 2
                  AND json_type(tag.value, '$[0]') = 'text'
                  AND json_extract(tag.value, '$[0]') = 'request.id'
                  AND json_type(tag.value, '$[1]') = 'text'
                THEN json_extract(tag.value, '$[1]') END
            WHEN 'object' THEN CASE
                WHEN json_type(tag.value, '$.key') = 'text'
                  AND json_extract(tag.value, '$.key') = 'request.id'
                  AND json_type(tag.value, '$.value') = 'text'
                THEN json_extract(tag.value, '$.value') END
        END AS request_id
    FROM events e, json_each(CASE json_type(e.data, '$.tags')
        WHEN 'object' THEN json_array(json_array('request.id', json_extract(e.data, '$.tags."request.id"')))
        WHEN 'array' THEN json_extract(e.data, '$.tags')
        ELSE '[]'
    END) AS tag
)
WHERE length(CAST(request_id AS BLOB)) BETWEEN 1 AND 200;

CREATE TRIGGER events_request_identity_insert AFTER INSERT ON events
BEGIN
    INSERT INTO event_request_identities (event_id, project_id, timestamp, request_id)
    SELECT event_id, project_id, timestamp, request_id
    FROM event_request_identity_values WHERE event_id = NEW.id;
END;
CREATE TRIGGER events_request_identity_update
AFTER UPDATE OF id, data, project_id, timestamp ON events
BEGIN
    DELETE FROM event_request_identities WHERE event_id IN (OLD.id, NEW.id);
    INSERT INTO event_request_identities (event_id, project_id, timestamp, request_id)
    SELECT event_id, project_id, timestamp, request_id
    FROM event_request_identity_values WHERE event_id = NEW.id;
END;

INSERT INTO event_request_identities (event_id, project_id, timestamp, request_id)
SELECT event_id, project_id, timestamp, request_id FROM event_request_identity_values;
