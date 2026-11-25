-- Every page someone viewed. `created_at` (unix seconds) makes it history:
-- how long it is kept is set in Rustrak's Storage page.
CREATE TABLE pageviews (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL,
    -- A daily-salted hash of the visitor: counts them without cookies, and
    -- cannot be turned back into an IP or followed from one day to the next.
    visitor TEXT NOT NULL,
    path TEXT NOT NULL,
    query TEXT,
    title TEXT,
    referrer TEXT,
    utm_source TEXT,
    utm_medium TEXT,
    utm_campaign TEXT,
    browser TEXT,
    os TEXT,
    device TEXT,
    country TEXT,
    language TEXT,
    created_at INTEGER NOT NULL
);
CREATE INDEX pageviews_by_time ON pageviews (project_id, created_at);
CREATE INDEX pageviews_by_path ON pageviews (project_id, path, created_at);

-- What a website reports with webTraffic.track(): a signup, a purchase.
CREATE TABLE events (
    id INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL,
    visitor TEXT NOT NULL,
    name TEXT NOT NULL,
    path TEXT NOT NULL,
    data TEXT,
    created_at INTEGER NOT NULL
);
CREATE INDEX events_by_time ON events (project_id, created_at);

-- Today's salt only: yesterday's is deleted the first time a new day's
-- visitor arrives, so no hash can be recomputed afterwards. No created_at:
-- retention never touches it.
CREATE TABLE salts (
    day TEXT PRIMARY KEY,
    salt TEXT NOT NULL
);
