-- A release's commits, in the order the client sent them (newest first, as
-- sentry-cli reads git), Sentry's ReleaseCommit. The first is the release's
-- last commit. One list per project release, deleted with it.
CREATE TABLE release_commits (
    release_id INTEGER NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    sha VARCHAR(64) NOT NULL,
    repository VARCHAR(64),
    author_name VARCHAR(128),
    author_email VARCHAR(200),
    message TEXT,
    date_created TEXT,
    PRIMARY KEY (release_id, position)
);
