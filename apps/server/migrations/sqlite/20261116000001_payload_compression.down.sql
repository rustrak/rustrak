-- Irreversible once a payload is stored: SQL cannot decompress `payload`
-- back into `data`, so dropping the column would destroy every row written
-- since. It refuses instead (SQLite's RAISE only works in triggers: the
-- failed CHECK names the reason); restore a backup taken before the upgrade.
-- A database with no compressed row (a fresh or test one) rolls back as usual.
CREATE TEMP TABLE payload_compression_guard (
    compressed INTEGER
        CONSTRAINT payload_compression_cannot_be_rolled_back_restore_a_backup
        CHECK (compressed = 0)
);
INSERT INTO payload_compression_guard SELECT
    EXISTS (SELECT 1 FROM events WHERE payload IS NOT NULL)
    + EXISTS (SELECT 1 FROM transactions WHERE payload IS NOT NULL)
    + EXISTS (SELECT 1 FROM spans WHERE payload IS NOT NULL);
DROP TABLE payload_compression_guard;
DROP TABLE payload_dictionaries;
ALTER TABLE spans DROP COLUMN payload;
ALTER TABLE transactions DROP COLUMN payload;
ALTER TABLE events DROP COLUMN payload;
