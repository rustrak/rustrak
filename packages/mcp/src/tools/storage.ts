import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import type { RustrakClient } from '@rustrak/client';
import { z } from 'zod';
import { mcpJson, mcpRefusal } from '../errors.js';

export function registerStorageTools(
  server: McpServer,
  client: RustrakClient,
): void {
  server.registerTool(
    'get_storage_summary',
    {
      description:
        'Get the instance-wide Rustrak storage summary (admin only): total database size, row counts for events, transactions, spans and logs, and the exact source-map weight. Use this to see what is consuming storage before cleaning up.',
      inputSchema: {},
    },
    async () => {
      const result = await client.storage.getSummary();
      return mcpJson(result);
    },
  );

  server.registerTool(
    'get_storage_by_project',
    {
      description:
        'Get the per-project Rustrak storage breakdown (admin only): event/transaction/span/source-map counts and estimated bytes for every project. Use this to find which project is accumulating the most data.',
      inputSchema: {},
    },
    async () => {
      const result = await client.storage.getProjects();
      return mcpJson(result);
    },
  );

  server.registerTool(
    'preview_storage_cleanup',
    {
      description:
        'Dry-run a Rustrak retention cleanup (admin only): report how many events, transactions, spans, logs and issues would be removed if data older than `older_than_days` were deleted, optionally scoped to one project and to specific data categories. Mutates nothing — always run this before execute_storage_cleanup.',
      inputSchema: {
        older_than_days: z
          .number()
          .int()
          .min(1)
          .describe('Delete data older than this many days'),
        project_id: z
          .number()
          .int()
          .optional()
          .describe('Scope to one project (omit for all projects)'),
        include_events: z
          .boolean()
          .optional()
          .describe(
            'Include error events (and the issues they empty). Defaults to true.',
          ),
        include_transactions: z
          .boolean()
          .optional()
          .describe(
            'Include transactions and their cascaded spans. Defaults to true.',
          ),
        include_logs: z
          .boolean()
          .optional()
          .describe('Include logs. Defaults to true.'),
      },
    },
    async ({
      older_than_days,
      project_id,
      include_events,
      include_transactions,
      include_logs,
    }) => {
      const result = await client.storage.previewCleanup({
        older_than_days,
        project_id,
        include_events,
        include_transactions,
        include_logs,
      });
      return mcpJson(result);
    },
  );

  server.registerTool(
    'execute_storage_cleanup',
    {
      description:
        'DESTRUCTIVE (admin only): permanently delete Rustrak data older than `older_than_days` (optionally scoped to one project and to specific data categories) and remove the issues it leaves with zero events. This cannot be undone. You MUST set confirm=true to proceed; without it the tool refuses and returns an error. Always run preview_storage_cleanup first and show the user the counts before confirming. The cleanup runs in the background: this returns the started job at once, and get_storage_cleanup_status reports its progress and final counts. Only one cleanup runs at a time; starting another while one runs is refused with a conflict.',
      inputSchema: {
        older_than_days: z
          .number()
          .int()
          .min(1)
          .describe('Delete data older than this many days'),
        project_id: z
          .number()
          .int()
          .optional()
          .describe('Scope to one project (omit for all projects)'),
        include_events: z
          .boolean()
          .optional()
          .describe(
            'Include error events (and the issues they empty). Defaults to true.',
          ),
        include_transactions: z
          .boolean()
          .optional()
          .describe(
            'Include transactions and their cascaded spans. Defaults to true.',
          ),
        include_logs: z
          .boolean()
          .optional()
          .describe('Include logs. Defaults to true.'),
        confirm: z
          .boolean()
          .optional()
          .describe('Must be true to run this destructive cleanup'),
      },
      annotations: { destructiveHint: true },
    },
    async ({
      older_than_days,
      project_id,
      include_events,
      include_transactions,
      include_logs,
      confirm,
    }) => {
      if (confirm !== true) {
        return mcpRefusal(
          'execute_storage_cleanup is destructive and was not confirmed. Run preview_storage_cleanup first, then call again with confirm=true to proceed.',
        );
      }
      const result = await client.storage.executeCleanup({
        older_than_days,
        project_id,
        include_events,
        include_transactions,
        include_logs,
      });
      return mcpJson(result);
    },
  );

  server.registerTool(
    'get_storage_cleanup_status',
    {
      description:
        'Get the progress of the running Rustrak storage cleanup, or the outcome of the last one (admin only): its state (idle, running, completed, failed), the rows removed so far, and when it started and finished. Use it after execute_storage_cleanup to tell the user when the deletion is done.',
      inputSchema: {},
    },
    async () => {
      const result = await client.storage.getCleanupStatus();
      return mcpJson(result);
    },
  );

  server.registerTool(
    'preview_storage_source_maps_gc',
    {
      description:
        'Dry-run a Rustrak source-map garbage collection (admin only): report how many orphaned source-map files would be removed and how many bytes reclaimed, without deleting anything. Mutates nothing — always run this before gc_storage_source_maps.',
      inputSchema: {},
    },
    async () => {
      const result = await client.storage.previewGcSourceMaps();
      return mcpJson(result);
    },
  );

  server.registerTool(
    'gc_storage_source_maps',
    {
      description:
        'DESTRUCTIVE (admin only): permanently remove orphaned Rustrak source-map files no longer referenced by any upload (e.g. left behind by deleted projects), from the database and disk. This cannot be undone. You MUST set confirm=true to proceed; without it the tool refuses and returns an error. Always run preview_storage_source_maps_gc first and show the user the counts before confirming.',
      inputSchema: {
        confirm: z
          .boolean()
          .optional()
          .describe('Must be true to run this destructive cleanup'),
      },
      annotations: { destructiveHint: true },
    },
    async ({ confirm }) => {
      if (confirm !== true) {
        return mcpRefusal(
          'gc_storage_source_maps is destructive and was not confirmed. Run preview_storage_source_maps_gc first, then call again with confirm=true to proceed.',
        );
      }
      const result = await client.storage.gcSourceMaps();
      return mcpJson(result);
    },
  );
}
