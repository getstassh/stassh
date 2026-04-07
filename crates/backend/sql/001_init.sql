CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    applied_at_unix_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS app_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    enable_telemetry INTEGER,
    telemetry_uuid TEXT,
    last_telemetry_report_at_unix_ms INTEGER,
    db_encryption TEXT,
    show_debug_panel INTEGER NOT NULL,
    ssh_idle_timeout_seconds INTEGER NOT NULL,
    ssh_connect_timeout_seconds INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS app_meta (
    key TEXT PRIMARY KEY,
    json_value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS hosts (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    user TEXT NOT NULL,
    auth_kind TEXT NOT NULL,
    auth_json_value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS host_endpoints (
    host_id INTEGER NOT NULL,
    endpoint_index INTEGER NOT NULL,
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    PRIMARY KEY (host_id, endpoint_index),
    FOREIGN KEY (host_id) REFERENCES hosts(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS trusted_host_keys (
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    algorithm TEXT NOT NULL,
    public_key_base64 TEXT NOT NULL,
    fingerprint_sha256 TEXT NOT NULL,
    PRIMARY KEY (host, port, algorithm, public_key_base64)
);
