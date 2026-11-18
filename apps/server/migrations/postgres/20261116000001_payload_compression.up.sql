-- Payloads compressed by the server (issue #532, `src/db/payload.rs`). A new
-- row keeps its JSON in `payload`, zstd-compressed behind a one-byte format,
-- and NULL in `data`. Rows written before keep their JSONB in `data` and stay
-- as they are.
--
-- Metadata only, no row rewritten or scanned: a nullable column without a
-- default, DROP NOT NULL and SET STORAGE each just edit the catalog, under
-- an ACCESS EXCLUSIVE lock held for that instant (it waits behind any
-- running query on the table). EXTERNAL keeps a large payload out of line
-- without TOAST trying to compress it again.
ALTER TABLE events ADD COLUMN payload BYTEA;
ALTER TABLE events ALTER COLUMN payload SET STORAGE EXTERNAL;
ALTER TABLE events ALTER COLUMN data DROP NOT NULL;
ALTER TABLE transactions ADD COLUMN payload BYTEA;
ALTER TABLE transactions ALTER COLUMN payload SET STORAGE EXTERNAL;
ALTER TABLE transactions ALTER COLUMN data DROP NOT NULL;
ALTER TABLE spans ADD COLUMN payload BYTEA;
ALTER TABLE spans ALTER COLUMN payload SET STORAGE EXTERNAL;
ALTER TABLE spans ALTER COLUMN data DROP NOT NULL;

-- Every dictionary ever trained, never deleted: rows name theirs by `id`,
-- a hash of its bytes. `kind` is the table it was trained on, `row_count`
-- that table's rows then, which the next training is measured against.
CREATE TABLE payload_dictionaries (
    id         BIGINT PRIMARY KEY,
    kind       TEXT NOT NULL,
    dictionary BYTEA NOT NULL,
    row_count  BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);
