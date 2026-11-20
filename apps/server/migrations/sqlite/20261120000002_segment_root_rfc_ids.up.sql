-- See the Postgres migration of the same version. Ids are 16-byte blobs: the
-- version is the 13th hex digit, the variant the 17th.
UPDATE transactions
SET id = f.fixed, event_id = f.fixed
FROM (
    SELECT id,
           unhex(substr(h, 1, 12) || '8' || substr(h, 14, 3)
                 || substr('89AB89AB89AB89AB', instr('0123456789ABCDEF', substr(h, 17, 1)), 1)
                 || substr(h, 18)) AS fixed
    FROM (SELECT id, hex(id) AS h FROM transactions
          WHERE id = event_id AND typeof(id) = 'blob' AND length(id) = 16) AS s
) AS f
WHERE transactions.id = f.id AND f.fixed <> f.id;
