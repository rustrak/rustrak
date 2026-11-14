-- Whether the reader wants a 24-hour clock in the dashboard.
--
-- Nullable on purpose, like `language` and `timezone`: NULL means "has not
-- chosen", and the dashboard then keeps the clock of the reader's language
-- (24-hour in French, 12-hour in English). FALSE is a reader who asked for the
-- 12-hour clock whatever their language.
ALTER TABLE users ADD COLUMN clock_24h BOOLEAN;
