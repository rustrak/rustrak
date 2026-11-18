-- Payloads compressed by the server (issue #532, `src/db/payload.rs`). A new
-- row keeps its JSON in `payload`, zstd-compressed behind a one-byte format,
-- and `data` holds the JSON `null`: SQLite cannot drop a NOT NULL without
-- rebuilding the table. Rows written before keep their JSON in `data` and
-- stay as they are. ADD COLUMN only edits the schema: no row is rewritten.
ALTER TABLE events ADD COLUMN payload BLOB;
ALTER TABLE transactions ADD COLUMN payload BLOB;
ALTER TABLE spans ADD COLUMN payload BLOB;

-- Every dictionary ever trained, never deleted: rows name theirs by `id`,
-- a hash of its bytes. `kind` is the table it was trained on, `row_count`
-- that table's rows then, which the next training is measured against.
CREATE TABLE payload_dictionaries (
    id         INTEGER PRIMARY KEY,
    kind       TEXT NOT NULL,
    dictionary BLOB NOT NULL,
    row_count  INTEGER NOT NULL,
    created_at TIMESTAMP NOT NULL
);
