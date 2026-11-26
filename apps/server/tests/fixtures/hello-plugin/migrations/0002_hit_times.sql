ALTER TABLE hits ADD COLUMN created_at INTEGER NOT NULL DEFAULT 0;
CREATE INDEX hits_by_project ON hits (project_id, id);
