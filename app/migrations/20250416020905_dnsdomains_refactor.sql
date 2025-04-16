-- Add migration script here
-- Drop and recreate the dnsdomains table to use (domain_id, provider, name) as the primary key and remove the id column
DROP TABLE IF EXISTS dnsdomains;

CREATE TABLE dnsdomains (
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    external_id TEXT,
    metadata JSON,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (provider, name)
);
