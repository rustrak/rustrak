-- Irreversible once a payload is stored: SQL cannot decompress `payload`
-- back into `data`, so dropping the column would destroy every row written
-- since, and `data` cannot be NOT NULL again while those rows have none.
-- It refuses instead; restore a backup taken before the upgrade. A database
-- with no compressed row (a fresh or test one) rolls back as usual.
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM events WHERE payload IS NOT NULL)
        OR EXISTS (SELECT 1 FROM transactions WHERE payload IS NOT NULL)
        OR EXISTS (SELECT 1 FROM spans WHERE payload IS NOT NULL) THEN
        RAISE EXCEPTION 'payload_compression cannot be rolled back: rows hold compressed payloads that SQL cannot decompress; restore a backup taken before the upgrade';
    END IF;
END $$;
DROP TABLE payload_dictionaries;
ALTER TABLE spans DROP COLUMN payload;
ALTER TABLE spans ALTER COLUMN data SET NOT NULL;
ALTER TABLE transactions DROP COLUMN payload;
ALTER TABLE transactions ALTER COLUMN data SET NOT NULL;
ALTER TABLE events DROP COLUMN payload;
ALTER TABLE events ALTER COLUMN data SET NOT NULL;
