-- Span indexes no query reads (issue #532). Every trace lookup also filters
-- by project, which idx_spans_project_trace and the gen_ai partials serve;
-- nothing looks a span up by span_id, or a transaction by trace_id.
--
-- Plain, transactional statements, not CONCURRENTLY (see
-- 20261112000000_events_grouping_index): migrations run before the server
-- listens and one process owns the database, so nothing writes meanwhile,
-- and a build that fails rolls the drops back with it instead of leaving an
-- INVALID partial index behind the dropped full one.
DROP INDEX IF EXISTS idx_spans_span_id;
DROP INDEX IF EXISTS idx_spans_trace;
DROP INDEX IF EXISTS idx_transactions_trace;

-- The gen_ai indexes held an entry for every span, AI or not. Every query
-- that reads them filters gen_ai_operation_type (`=`, `IN` or IS NOT NULL),
-- which implies this predicate, so the planner still picks them.
DROP INDEX IF EXISTS idx_spans_project_gen_ai_op_type;
DROP INDEX IF EXISTS idx_spans_project_gen_ai_model;
DROP INDEX IF EXISTS idx_spans_project_gen_ai_tool;
CREATE INDEX IF NOT EXISTS idx_spans_gen_ai_op_type
    ON spans (project_id, gen_ai_operation_type) WHERE gen_ai_operation_type IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_spans_gen_ai_model
    ON spans (project_id, gen_ai_response_model) WHERE gen_ai_operation_type IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_spans_gen_ai_tool
    ON spans (project_id, gen_ai_tool_name) WHERE gen_ai_operation_type IS NOT NULL;
