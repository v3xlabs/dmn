--- Add auto_renew field to domains table
ALTER TABLE domains ADD COLUMN ext_auto_renew BOOLEAN;
