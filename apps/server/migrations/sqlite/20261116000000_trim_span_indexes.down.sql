DROP INDEX IF EXISTS idx_spans_gen_ai_tool;
DROP INDEX IF EXISTS idx_spans_gen_ai_model;
DROP INDEX IF EXISTS idx_spans_gen_ai_op_type;
CREATE INDEX IF NOT EXISTS idx_spans_project_gen_ai_tool ON spans(project_id, gen_ai_tool_name);
CREATE INDEX IF NOT EXISTS idx_spans_project_gen_ai_model ON spans(project_id, gen_ai_response_model);
CREATE INDEX IF NOT EXISTS idx_spans_project_gen_ai_op_type ON spans(project_id, gen_ai_operation_type);
CREATE INDEX IF NOT EXISTS idx_transactions_trace ON transactions(trace_id);
CREATE INDEX IF NOT EXISTS idx_spans_trace ON spans(trace_id);
CREATE INDEX IF NOT EXISTS idx_spans_span_id ON spans(span_id);
