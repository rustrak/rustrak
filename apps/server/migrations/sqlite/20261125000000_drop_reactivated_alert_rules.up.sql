-- "Reactivated" (`unmute`) rules waited for an archived issue to come back,
-- which Rustrak never does on its own, so they could never fire (#494). The
-- type is no longer accepted; its rules are deleted. Their history stays as a
-- record, with alert_rule_id set to NULL (ON DELETE SET NULL).
-- The column's CHECK still lists 'unmute', which nothing writes any more.
DELETE FROM alert_rules WHERE alert_type = 'unmute';
