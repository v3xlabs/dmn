-- Add migration script here
DROP TABLE IF EXISTS dnsdomains;
DROP TABLE IF EXISTS dns_domains;

CREATE TABLE dns_domains (
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    external_id TEXT,
    metadata JSON,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (provider, name)
);
