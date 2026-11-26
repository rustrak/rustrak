-- Installed plugins. The package and the plugin's own SQLite file live on
-- disk under RUSTRAK_PLUGINS_DIR; this says what is installed, in which
-- state, and where it runs.
CREATE TABLE plugins (
    id TEXT PRIMARY KEY,
    version TEXT NOT NULL,
    state VARCHAR(24) NOT NULL DEFAULT 'enabled',
    -- 'registry' (signed, verified) or 'upload' (by hand).
    source VARCHAR(16) NOT NULL DEFAULT 'upload',
    -- The instance's values of the plugin's settings (its manifest's schema).
    settings JSONB NOT NULL DEFAULT '{}',
    -- Active in every project, the ones created later included.
    all_projects BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- The projects a plugin was turned on in one by one.
CREATE TABLE plugin_projects (
    plugin_id TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    PRIMARY KEY (plugin_id, project_id)
);

-- A project's values of the plugin's project settings. Apart from
-- plugin_projects, so choosing where it runs never loses them.
CREATE TABLE plugin_project_settings (
    plugin_id TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    project_id INTEGER NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    settings JSONB NOT NULL DEFAULT '{}',
    PRIMARY KEY (plugin_id, project_id)
);
