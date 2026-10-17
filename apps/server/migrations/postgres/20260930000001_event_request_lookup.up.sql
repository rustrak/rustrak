-- Match SQLite's materialized request tags without rewriting raw SDK payloads.
-- Table, triggers and historical backfill commit together in this migration.
CREATE TABLE event_request_identities (
    event_id UUID NOT NULL REFERENCES events(id) ON DELETE CASCADE ON UPDATE CASCADE,
    request_id TEXT NOT NULL CHECK (octet_length(request_id) BETWEEN 1 AND 200),
    project_id INTEGER NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (event_id, request_id)
);
CREATE INDEX idx_events_project_request_identity
    ON event_request_identities (project_id, request_id, timestamp DESC, event_id DESC);

-- One extraction definition serves historical backfill and future writes.
-- CASE protects array-length checks from malformed/scalar entries.
CREATE VIEW event_request_identity_values AS
SELECT DISTINCT e.id AS event_id, e.project_id, e.timestamp, identity.request_id
FROM events e
CROSS JOIN LATERAL jsonb_array_elements(CASE jsonb_typeof(e.data -> 'tags')
    WHEN 'object' THEN jsonb_build_array(jsonb_build_array('request.id', e.data #> '{tags,request.id}'))
    WHEN 'array' THEN e.data -> 'tags'
    ELSE '[]'::jsonb
END) AS tag(value)
CROSS JOIN LATERAL (SELECT CASE jsonb_typeof(tag.value)
    WHEN 'array' THEN CASE
        WHEN jsonb_array_length(tag.value) = 2
          AND jsonb_typeof(tag.value -> 0) = 'string'
          AND tag.value ->> 0 = 'request.id'
          AND jsonb_typeof(tag.value -> 1) = 'string'
        THEN tag.value ->> 1 END
    WHEN 'object' THEN CASE
        WHEN jsonb_typeof(tag.value -> 'key') = 'string'
          AND tag.value ->> 'key' = 'request.id'
          AND jsonb_typeof(tag.value -> 'value') = 'string'
        THEN tag.value ->> 'value' END
END AS request_id) AS identity
WHERE octet_length(identity.request_id) BETWEEN 1 AND 200;

CREATE FUNCTION sync_event_request_identities() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'UPDATE' THEN
        DELETE FROM event_request_identities WHERE event_id IN (OLD.id, NEW.id);
    END IF;
    INSERT INTO event_request_identities (event_id, project_id, timestamp, request_id)
    SELECT event_id, project_id, timestamp, request_id
    FROM event_request_identity_values WHERE event_id = NEW.id;
    RETURN NEW;
END;
$$;
CREATE TRIGGER events_request_identity_write
AFTER INSERT OR UPDATE OF id, data, project_id, timestamp ON events
FOR EACH ROW EXECUTE FUNCTION sync_event_request_identities();

INSERT INTO event_request_identities (event_id, project_id, timestamp, request_id)
SELECT event_id, project_id, timestamp, request_id FROM event_request_identity_values;
