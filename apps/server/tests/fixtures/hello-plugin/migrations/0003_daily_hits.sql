-- Hits per project and day. No created_at: Rustrak's retention keeps it
-- whatever it clears from hits.
CREATE TABLE daily_hits (
    project_id INTEGER NOT NULL,
    day TEXT NOT NULL,
    hits INTEGER NOT NULL,
    PRIMARY KEY (project_id, day)
);
