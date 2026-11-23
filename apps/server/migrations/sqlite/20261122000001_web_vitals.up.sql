-- One row per web vital measured on a page, scored as Relay scores it
-- (services/web_vitals.rs). Kept apart from transactions because most
-- vitals arrive on spans of their own, after the page load they belong to.
CREATE TABLE web_vitals (
    id             INTEGER PRIMARY KEY,
    project_id     INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    page           VARCHAR(200) NOT NULL,
    vital          VARCHAR(8) NOT NULL,
    value          REAL NOT NULL,
    score          REAL NOT NULL,
    browser        VARCHAR(64),
    environment    VARCHAR(64),
    release        VARCHAR(200),
    -- What the vital is about: the LCP's element, the CLS's first shifted
    -- element, the INP's interaction target.
    target         TEXT,
    -- The resource the LCP element loaded (an image's URL).
    resource       TEXT,
    -- The page's own URL, for a lab test of it.
    url            TEXT,
    device         VARCHAR(8),
    -- The network the browser reported (`4g`, `3g`...).
    connection     VARCHAR(16),
    trace_id       TEXT,
    span_id        TEXT NOT NULL,
    -- The page load's own span: the vitals of one page load share it,
    -- whichever span they came on.
    pageload_span_id TEXT,
    -- The page load's transaction, when it is known.
    transaction_id TEXT,
    timestamp      TEXT NOT NULL
);

-- The span a vital came on, so the same span delivered again adds nothing.
CREATE UNIQUE INDEX idx_web_vitals_span ON web_vitals(project_id, span_id, vital);
CREATE INDEX idx_web_vitals_window ON web_vitals(project_id, timestamp);
