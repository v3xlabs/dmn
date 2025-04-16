-- Drop and recreate the domains table to use (provider, name) as the primary key and remove the id column
DROP TABLE IF EXISTS domains;

CREATE TABLE domains (
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    external_id TEXT,
    metadata JSON,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (provider, name)
);
