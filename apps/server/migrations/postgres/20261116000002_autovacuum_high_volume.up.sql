-- Vacuum the tables retention deletes from at 1% dead rows instead of 20%
-- (issue #532). Autovacuum vacuums a table once its dead rows pass
-- autovacuum_vacuum_threshold + autovacuum_vacuum_scale_factor * reltuples,
-- 50 + 0.2 * rows by default, and analyzes it past 50 + 0.1 * rows changed
-- (PostgreSQL docs, "Routine Vacuuming", 24.1.6 The Autovacuum Daemon). At
-- tens of millions of rows the default waits for millions of dead rows, and
-- measured on a purge, tables grew 18% and indexes 29% when vacuum lagged
-- against 4% when it ran promptly. Both are per-table storage parameters
-- (CREATE TABLE, "Storage Parameters"); the analyze one also keeps the row
-- estimate the payload dictionaries are trained by current.
--
-- Metadata only: setting autovacuum parameters takes a SHARE UPDATE
-- EXCLUSIVE lock, which reads and writes go on under (ALTER TABLE docs).
ALTER TABLE events SET (autovacuum_vacuum_scale_factor = 0.01, autovacuum_analyze_scale_factor = 0.01);
ALTER TABLE transactions SET (autovacuum_vacuum_scale_factor = 0.01, autovacuum_analyze_scale_factor = 0.01);
ALTER TABLE spans SET (autovacuum_vacuum_scale_factor = 0.01, autovacuum_analyze_scale_factor = 0.01);
ALTER TABLE logs SET (autovacuum_vacuum_scale_factor = 0.01, autovacuum_analyze_scale_factor = 0.01);
