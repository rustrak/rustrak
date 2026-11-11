/**
 * Exercises Rustrak's AI Agent Monitoring (GH #180) end-to-end with the REAL
 * `@sentry/node` `vercelAIIntegration()` and the REAL Vercel AI SDK
 * (`generateText` with a tool + a multi-step agent loop) — not a hand-rolled
 * envelope. This is the true compatibility test: if Rustrak's gen_ai.*
 * normalization (services/gen_ai.rs) recognizes what the real SDKs emit, a
 * production AI agent can't tell the difference.
 *
 * No live LLM call is made — `MockLanguageModelV3` (from `ai/test`) stands in
 * for the provider so this runs with no API key and no network access, while
 * still driving `generateText` through its real OpenTelemetry-instrumented
 * code path (tool call → tool result → final answer).
 *
 * Usage:
 *   SENTRY_DSN=http://<sentry_key>@localhost:8080/<project_id> pnpm demo:ai-agent
 * Default DSN targets project 1 on a local server.
 *
 * Sentry must be initialized before `ai` is imported (see
 * ai-agent-instrument.ts) — that's why this file takes no Sentry import
 * itself and is run via `tsx --import demo/src/ai-agent-instrument.ts`.
 */
import { generateText, stepCountIs, tool } from 'ai';
import { MockLanguageModelV3 } from 'ai/test';
import { z } from 'zod';

const webSearchTool = tool({
  description: 'Search the web for up-to-date information',
  inputSchema: z.object({
    query: z.string().describe('The search query'),
  }),
  execute: async ({ query }) => {
    console.log(`  [tool] web_search("${query}")`);
    return {
      results: [
        {
          title: 'Rust async/await guide',
          snippet: 'Rust async functions compile to state machines...',
        },
      ],
    };
  },
});

const model = new MockLanguageModelV3({
  modelId: 'gpt-4o',
  doGenerate: [
    // Step 1: model decides to call the tool.
    {
      finishReason: { unified: 'tool-calls', raw: undefined },
      warnings: [],
      usage: {
        inputTokens: {
          total: 210,
          noCache: 210,
          cacheRead: undefined,
          cacheWrite: undefined,
        },
        outputTokens: { total: 34, text: 34, reasoning: undefined },
      },
      content: [
        {
          type: 'tool-call',
          toolCallId: 'call_1',
          toolName: 'webSearch',
          input: JSON.stringify({ query: 'rust async await' }),
        },
      ],
    },
    // Step 2: model produces the final answer using the tool result.
    {
      finishReason: { unified: 'stop', raw: undefined },
      warnings: [],
      usage: {
        inputTokens: {
          total: 302,
          noCache: 302,
          cacheRead: undefined,
          cacheWrite: undefined,
        },
        outputTokens: { total: 96, text: 96, reasoning: undefined },
      },
      content: [
        {
          type: 'text',
          text: 'Rust async functions compile down to state machines driven by an executor — no green threads, no runtime by default.',
        },
      ],
    },
  ],
});

async function main() {
  console.log('Running AI agent demo (research_agent) via Vercel AI SDK...\n');

  const result = await generateText({
    model,
    tools: { webSearch: webSearchTool },
    stopWhen: stepCountIs(3),
    prompt: 'Research how Rust implements async/await and summarize it.',
    experimental_telemetry: {
      isEnabled: true,
      functionId: 'research_agent',
    },
  });

  console.log(`\nFinal answer: ${result.text}`);
  console.log(`Steps: ${result.steps.length}`);

  const Sentry = await import('@sentry/node');
  await Sentry.flush(5000);
  console.log('\n✓ Flushed. Open the Agents tab to see the trace.');
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
