ALTER TABLE events RESET (autovacuum_vacuum_scale_factor, autovacuum_analyze_scale_factor);
ALTER TABLE transactions RESET (autovacuum_vacuum_scale_factor, autovacuum_analyze_scale_factor);
ALTER TABLE spans RESET (autovacuum_vacuum_scale_factor, autovacuum_analyze_scale_factor);
ALTER TABLE logs RESET (autovacuum_vacuum_scale_factor, autovacuum_analyze_scale_factor);
