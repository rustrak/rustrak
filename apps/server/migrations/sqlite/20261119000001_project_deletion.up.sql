-- A project being deleted (issue #556). The request sets this and answers;
-- its rows are removed afterwards, in batches, and the project row last.
-- Every reader treats a project with it set as absent.
ALTER TABLE projects ADD COLUMN deleted_at TEXT;
