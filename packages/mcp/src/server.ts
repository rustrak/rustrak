import { readFileSync } from 'node:fs';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import type { RustrakClient } from '@rustrak/client';
import { registerAgentTools } from './tools/agents.js';
import { registerAlertTools } from './tools/alerts.js';
import { registerEventTools } from './tools/events.js';
import { registerHealthTools } from './tools/health.js';
import { registerIssueTools } from './tools/issues.js';
import { registerLogTools } from './tools/logs.js';
import { registerMonitorTools } from './tools/monitors.js';
import { registerProjectTools } from './tools/projects.js';
import { registerScheduleTools } from './tools/schedules.js';
import { registerSessionTools } from './tools/sessions.js';
import { registerSpanTools } from './tools/spans.js';
import { registerStatsTools } from './tools/stats.js';
import { registerStorageTools } from './tools/storage.js';
import { registerTeamTools } from './tools/team.js';
import { registerTokenTools } from './tools/tokens.js';
import { registerTransactionTools } from './tools/transactions.js';

/**
 * The version advertised in the MCP handshake, read from package.json.
 *
 * It is derived rather than written down because `packages/mcp` is in the
 * `fixed` group in `.changeset/config.json`: every release bumps package.json,
 * and a second copy of the number would silently go stale between releases.
 *
 * `../package.json` resolves to the package root from both `src/server.ts` and
 * the bundled `dist/index.js`, since both sit one level down. npm always ships
 * package.json regardless of the `files` list, so it is there at runtime.
 */
const VERSION = (
  JSON.parse(
    readFileSync(new URL('../package.json', import.meta.url), 'utf8'),
  ) as { version: string }
).version;

export function createServer(client: RustrakClient): McpServer {
  const server = new McpServer({
    name: 'rustrak-mcp',
    version: VERSION,
  });

  registerProjectTools(server, client);
  registerIssueTools(server, client);
  registerEventTools(server, client);
  registerTokenTools(server, client);
  registerAlertTools(server, client);
  registerTeamTools(server, client);
  registerHealthTools(server, client);
  registerSessionTools(server, client);
  registerTransactionTools(server, client);
  registerLogTools(server, client);
  registerMonitorTools(server, client);
  registerScheduleTools(server, client);
  registerStorageTools(server, client);
  registerSpanTools(server, client);
  registerAgentTools(server, client);
  registerStatsTools(server, client);

  return server;
}
