import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import type { RustrakClient } from '@rustrak/client';
import { z } from 'zod';
import { mcpDone, mcpJson } from '../errors.js';
import { scheduleSchema } from './schedules.js';

/** A monitor_config, the shape Sentry SDKs send to create a monitor. */
const configSchema = z.object({
  schedule: scheduleSchema.optional(),
  checkin_margin: z
    .number()
    .int()
    .min(1)
    .optional()
    .describe('Minutes a check-in may be late before the run counts as missed'),
  max_runtime: z
    .number()
    .int()
    .min(1)
    .optional()
    .describe('Minutes a run may stay in progress before it times out'),
  timezone: z.string().optional().describe('IANA zone the crontab is read in'),
  failure_issue_threshold: z
    .number()
    .int()
    .min(1)
    .max(720)
    .optional()
    .describe('Consecutive failures before an issue opens'),
  recovery_threshold: z
    .number()
    .int()
    .min(1)
    .max(720)
    .optional()
    .describe('Consecutive successes before the issue resolves'),
});

export function registerMonitorTools(
  server: McpServer,
  client: RustrakClient,
): void {
  server.registerTool(
    'list_monitors',
    {
      description:
        'List the cron monitors of a project (Sentry Crons): each scheduled job with its schedule, config, ' +
        'and per-environment status (active = waiting for a first ok run, ok, error = incident open) with its recent check-ins.',
      inputSchema: { project_id: z.number().int().describe('Project ID') },
    },
    async ({ project_id }) => mcpJson(await client.monitors.list(project_id)),
  );

  server.registerTool(
    'get_monitor',
    {
      description:
        'Get one cron monitor by slug: its config, environments, last and next expected check-in, and recent check-ins.',
      inputSchema: {
        project_id: z.number().int().describe('Project ID'),
        slug: z.string().min(1).describe('Monitor slug'),
      },
    },
    async ({ project_id, slug }) =>
      mcpJson(await client.monitors.get(project_id, slug)),
  );

  server.registerTool(
    'list_monitor_checkins',
    {
      description:
        'List the check-ins of a cron monitor, newest first. status is ok, error, in_progress, missed, timeout or unknown; ' +
        'missed and timeout are set by Rustrak when a run did not report on time.',
      inputSchema: {
        project_id: z.number().int().describe('Project ID'),
        slug: z.string().min(1).describe('Monitor slug'),
        page: z.number().int().min(1).optional().describe('Page (default 1)'),
        per_page: z
          .number()
          .int()
          .min(1)
          .max(100)
          .optional()
          .describe('Items per page (default 50)'),
        environment: z.string().optional().describe('Only this environment'),
      },
    },
    async ({ project_id, slug, page, per_page, environment }) =>
      mcpJson(
        await client.monitors.listCheckIns(project_id, slug, {
          ...(page === undefined ? {} : { page }),
          ...(per_page === undefined ? {} : { per_page }),
          ...(environment === undefined ? {} : { environment }),
        }),
      ),
  );

  server.registerTool(
    'create_monitor',
    {
      description:
        'Create a cron monitor. Its slug (derived from the name when omitted) is what the job checks in with. ' +
        'Most monitors are instead created by the job itself, by sending a monitor_config with its first check-in.',
      inputSchema: {
        project_id: z.number().int().describe('Project ID'),
        name: z.string().min(1).max(128).describe('Display name'),
        slug: z
          .string()
          .min(1)
          .max(50)
          .optional()
          .describe('Slug; from the name when omitted'),
        config: configSchema.describe('Config; schedule is required'),
      },
    },
    async ({ project_id, name, slug, config }) =>
      mcpJson(
        await client.monitors.create(project_id, {
          name,
          ...(slug === undefined ? {} : { slug }),
          config,
        }),
      ),
  );

  server.registerTool(
    'update_monitor',
    {
      description:
        'Rename, reconfigure, mute or unmute a cron monitor. Config fields left out keep their value. ' +
        'A muted monitor still records check-ins but opens no issue and sends no alert.',
      inputSchema: {
        project_id: z.number().int().describe('Project ID'),
        slug: z.string().min(1).describe('Monitor slug'),
        name: z.string().min(1).max(128).optional().describe('New name'),
        config: configSchema.optional().describe('Config fields to change'),
        is_muted: z.boolean().optional().describe('Mute or unmute'),
      },
    },
    async ({ project_id, slug, name, config, is_muted }) =>
      mcpJson(
        await client.monitors.update(project_id, slug, {
          ...(name === undefined ? {} : { name }),
          ...(config === undefined ? {} : { config }),
          ...(is_muted === undefined ? {} : { is_muted }),
        }),
      ),
  );

  server.registerTool(
    'delete_monitor',
    {
      description:
        'Permanently delete a cron monitor and its check-ins. The issues it opened stay. ' +
        'A job that keeps checking in with a monitor_config creates it again.',
      inputSchema: {
        project_id: z.number().int().describe('Project ID'),
        slug: z.string().min(1).describe('Monitor slug'),
      },
      annotations: { destructiveHint: true },
    },
    async ({ project_id, slug }) =>
      mcpDone(
        await client.monitors.delete(project_id, slug),
        `Monitor ${slug} deleted.`,
      ),
  );
}
