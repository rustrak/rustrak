import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import type { RustrakClient } from '@rustrak/client';
import { z } from 'zod';
import { mcpJson } from '../errors.js';

/** A crontab or an interval: the schedule of a cron monitor or a server job. */
export const scheduleSchema = z.discriminatedUnion('type', [
  z.object({
    type: z.literal('crontab'),
    value: z
      .string()
      .describe('Five-field crontab or alias, e.g. "0 3 * * *" or "@daily"'),
  }),
  z.object({
    type: z.literal('interval'),
    value: z.number().int().min(1),
    unit: z.enum(['minute', 'hour', 'day', 'week', 'month', 'year']),
  }),
]);

export function registerScheduleTools(
  server: McpServer,
  client: RustrakClient,
): void {
  server.registerTool(
    'preview_schedule',
    {
      description:
        'Validate a schedule (crontab or interval) and list its next five runs, using the parser Rustrak runs it with. ' +
        'Use it before saving a cron monitor or the automatic cleanup schedule.',
      inputSchema: {
        schedule: scheduleSchema.describe('The schedule to preview'),
        timezone: z
          .string()
          .optional()
          .describe('IANA zone, e.g. Europe/Madrid (default UTC)'),
      },
    },
    async ({ schedule, timezone }) =>
      mcpJson(
        await client.schedules.preview({
          schedule,
          ...(timezone === undefined ? {} : { timezone }),
        }),
      ),
  );
}
