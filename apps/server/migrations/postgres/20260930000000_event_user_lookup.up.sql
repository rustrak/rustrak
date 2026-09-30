-- no-transaction
-- One concurrent index per migration lets ingestion continue during creation.
-- Bound stored identifiers by bytes so oversized SDK values cannot break inserts.
-- Do not hide an INVALID index from an interrupted build with IF NOT EXISTS.
CREATE INDEX CONCURRENTLY idx_events_project_user_identity
    ON events (project_id, (data #>> '{user,id}'), timestamp DESC, id DESC)
    WHERE jsonb_typeof(data #> '{user,id}') = 'string'
      AND octet_length(data #>> '{user,id}') BETWEEN 1 AND 200;
