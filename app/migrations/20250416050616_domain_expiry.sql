--- Add ext_expiry_at and ext_registered_at to domains table
ALTER TABLE domains ADD COLUMN ext_expiry_at TIMESTAMP WITH TIME ZONE;
ALTER TABLE domains ADD COLUMN ext_registered_at TIMESTAMP WITH TIME ZONE;
