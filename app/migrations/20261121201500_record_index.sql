-- Due time and failure state per tracked name and kind of record.
CREATE TABLE index_state (
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    next_check_at TIMESTAMP WITH TIME ZONE NOT NULL,
    last_checked_at TIMESTAMP WITH TIME ZONE,
    failures INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    PRIMARY KEY (name, kind)
);

-- One row per distinct set of normalized fields; an equal fetch only moves confirmed_at.
CREATE TABLE snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    source TEXT NOT NULL,
    server TEXT NOT NULL,
    fields JSON NOT NULL,
    raw TEXT NOT NULL,
    observed_at TIMESTAMP WITH TIME ZONE NOT NULL,
    confirmed_at TIMESTAMP WITH TIME ZONE NOT NULL
);
CREATE INDEX snapshots_latest ON snapshots (name, kind, id);

-- Append-only, one row per changed field; consumers read it by increasing id.
CREATE TABLE record_changes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    field TEXT NOT NULL,
    before JSON NOT NULL,
    after JSON NOT NULL,
    snapshot_id INTEGER NOT NULL REFERENCES snapshots (id),
    observed_at TIMESTAMP WITH TIME ZONE NOT NULL
);
CREATE INDEX record_changes_name ON record_changes (name, id);

-- When the indexer may query an RDAP or WHOIS host again, so a restart keeps a long Retry-After.
CREATE TABLE index_hosts (
    host TEXT PRIMARY KEY,
    free_at TIMESTAMP WITH TIME ZONE NOT NULL
);
