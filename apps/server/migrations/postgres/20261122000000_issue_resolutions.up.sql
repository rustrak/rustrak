-- Sentry's `GroupResolution`: which release an issue was resolved in, so an
-- event from an older release does not reopen it (GH #576). One row per
-- resolved issue; plain "resolve" writes none.
--   type: 0 in_release, 1 in_next_release (NULL: legacy, read as the latter)
--   status: 0 pending (waiting for the next release), 1 resolved
CREATE TABLE issue_resolutions (
    issue_id UUID PRIMARY KEY REFERENCES issues(id) ON DELETE CASCADE,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    release_id INTEGER NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
    type SMALLINT,
    status SMALLINT NOT NULL DEFAULT 0,
    current_release_version VARCHAR(200),
    actor_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
    datetime TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_issue_resolutions_project_status ON issue_resolutions(project_id, status);
CREATE INDEX idx_issue_resolutions_release ON issue_resolutions(release_id);

-- Sentry creates a release the first time an event names it
-- (`_get_or_create_release_many`); the digest does from now on. The releases
-- retained events and issues name, each dated by the earliest of what is
-- left: its events' timestamps and the issues first and last seen with it,
-- since retention may have taken the first events. A release CI registered
-- keeps its own date. Versions `is_valid_version` refuses are left out.
INSERT INTO releases (project_id, version, date_created)
SELECT project_id, version, MIN(seen)
FROM (
    SELECT project_id, release AS version, timestamp AS seen FROM events
    UNION ALL
    SELECT project_id, first_release, first_seen FROM issues
    UNION ALL
    SELECT project_id, last_release, last_seen FROM issues
) named
WHERE version <> '' AND LENGTH(version) <= 200
  AND version NOT IN ('.', '..') AND LOWER(version) <> 'latest'
  AND version !~ E'[/\\\\\\r\\n\\f\\t]'
GROUP BY project_id, version
ON CONFLICT (project_id, version) DO NOTHING;

-- The old marker becomes a pending in-next-release resolution anchored on
-- the issue's last release, which is what it compared against.
INSERT INTO issue_resolutions (issue_id, project_id, release_id, type, status, current_release_version, datetime)
SELECT i.id, i.project_id, r.id, 1, 0, i.last_release, i.last_seen
FROM issues i
JOIN releases r ON r.project_id = i.project_id AND r.version = i.last_release
WHERE i.status = 'resolved' AND i.status_details LIKE '%"in_next_release":true%';

-- `statusDetails` as Sentry's serializer writes it.
UPDATE issues
SET status_details = CASE
    WHEN id IN (SELECT issue_id FROM issue_resolutions) THEN '{"inNextRelease":true}'
    ELSE '{}'
END
WHERE status_details LIKE '%in_next_release%';
