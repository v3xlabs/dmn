-- Index keys use the ASCII part of `DomainName::from_str`: trim whitespace, strip one root dot, lower case.
-- Earlier versions stored the provider's spelling, such as ` Example.NET. `; of several spellings of one name, one index row stays.
CREATE TEMP TABLE canonical_names AS
SELECT name,
       CASE WHEN substr(trimmed, -1) = '.' THEN lower(substr(trimmed, 1, length(trimmed) - 1))
            ELSE lower(trimmed)
       END AS key
FROM (
    SELECT name, trim(name, ' ' || char(9, 10, 11, 12, 13)) AS trimmed
    FROM (
        SELECT name FROM index_state
        UNION SELECT name FROM snapshots
        UNION SELECT name FROM record_changes
    )
);

DELETE FROM index_state
WHERE rowid NOT IN (
    SELECT MIN(index_state.rowid)
    FROM index_state JOIN canonical_names USING (name)
    GROUP BY canonical_names.key, index_state.kind
);
UPDATE index_state SET name = (SELECT key FROM canonical_names WHERE canonical_names.name = index_state.name);
UPDATE snapshots SET name = (SELECT key FROM canonical_names WHERE canonical_names.name = snapshots.name);
UPDATE record_changes SET name = (SELECT key FROM canonical_names WHERE canonical_names.name = record_changes.name);

DROP TABLE canonical_names;
