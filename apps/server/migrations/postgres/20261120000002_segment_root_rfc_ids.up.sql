-- A transaction made from a streamed segment root took a bare SHA-256 prefix
-- as its id, with random version and variant bits the client rejects (#572).
-- Such a row is the only kind whose id is its event_id. Setting the v8 and RFC
-- variant bits gives the id `SegmentRoot::id` derives now, so the same root
-- delivered again still matches it. No span references these ids.
UPDATE transactions
SET id = f.fixed, event_id = f.fixed
FROM (
    SELECT id,
           encode(set_byte(set_byte(b, 6, (get_byte(b, 6) & 15) | 128),
                           8, (get_byte(b, 8) & 63) | 128), 'hex')::uuid AS fixed
    FROM (SELECT id, uuid_send(id) AS b FROM transactions WHERE id = event_id) AS s
) AS f
WHERE transactions.id = f.id AND f.fixed <> f.id;
