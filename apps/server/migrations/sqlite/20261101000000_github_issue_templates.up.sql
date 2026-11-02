-- What every GitHub issue Rustrak opens starts with: labels, a title prefix
-- and a footer for the body (JSON). The instance's lives on the app, a
-- project's on its repository and goes on top of it.
ALTER TABLE github_app ADD COLUMN issue_template TEXT NOT NULL DEFAULT '{}';
ALTER TABLE project_repositories ADD COLUMN issue_template TEXT NOT NULL DEFAULT '{}';
