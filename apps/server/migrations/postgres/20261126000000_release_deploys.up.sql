-- A release going out to an environment, Sentry's Deploy
-- (`sentry-cli releases deploys new`). One row per project release, so
-- deleting the release deletes its deploys.
CREATE TABLE release_deploys (
    id SERIAL PRIMARY KEY,
    release_id INTEGER NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
    environment VARCHAR(64) NOT NULL,
    name VARCHAR(64),
    url TEXT,
    date_started TIMESTAMPTZ,
    date_finished TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_release_deploys_release ON release_deploys(release_id, environment, date_finished DESC);
