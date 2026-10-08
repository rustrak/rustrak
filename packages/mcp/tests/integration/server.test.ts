import { readFileSync } from 'node:fs';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createTestEnv } from '../setup.js';

const EXPECTED_TOOLS = [
  // Projects (3)
  'list_projects',
  'get_project',
  'create_project',
  // Issues (21)
  'list_issues',
  'get_issue',
  'resolve_issue',
  'unresolve_issue',
  'mute_issue',
  'delete_issue',
  'update_issue_status',
  'assign_issue',
  'bulk_update_issues',
  'bulk_delete_issues',
  'get_issue_hashes',
  'get_issue_tag_values',
  'get_issue_aggregates',
  'get_issue_stats',
  'get_issue_activity',
  'comment_on_issue',
  'bookmark_issue',
  'subscribe_issue',
  'mark_issue_seen',
  'list_user_reports',
  'submit_user_report',
  // Events (2)
  'list_events',
  'get_event',
  // Tokens (4)
  'list_tokens',
  'get_token',
  'create_token',
  'revoke_token',
  // Alerts (6)
  'list_alert_channels',
  'create_alert_channel',
  'update_alert_channel',
  'preview_alert_template',
  'test_alert_channel',
  'list_alert_rules',
  // Team (9)
  'list_team_members',
  'update_member_role',
  'remove_team_member',
  'create_invitation',
  'list_invitations',
  'revoke_invitation',
  'list_project_members',
  'set_project_member',
  'remove_project_member',
  // Health (1)
  'get_server_version',
  // Sessions (1)
  'get_release_health',
  // Transactions (4)
  'list_transactions',
  'get_transaction',
  'get_transaction_stats',
  'get_transaction_spans',
  // Logs (1)
  'list_logs',
  // Storage (6)
  'get_storage_summary',
  'get_storage_by_project',
  'preview_storage_cleanup',
  'execute_storage_cleanup',
  'get_storage_cleanup_status',
  'preview_storage_source_maps_gc',
  'gc_storage_source_maps',
  // Spans (1)
  'list_spans',
  // AI Agent Monitoring (6)
  'get_agent_runs',
  'get_agent_duration',
  'get_agent_models_by_calls',
  'get_agent_models_by_tokens',
  'get_agent_tools',
  'list_agent_traces',
  // Project stats (2)
  'get_error_volume',
  'get_project_stats',
] as const;

describe('MCP server integration', () => {
  let mockClient: any;
  let testEnv: Awaited<ReturnType<typeof createTestEnv>>;

  beforeEach(async () => {
    mockClient = {
      stats: { timeseries: vi.fn(), summary: vi.fn() },
      projects: { list: vi.fn(), get: vi.fn(), create: vi.fn() },
      issues: {
        list: vi.fn(),
        get: vi.fn(),
        updateState: vi.fn(),
        delete: vi.fn(),
      },
      events: { list: vi.fn(), get: vi.fn() },
      tokens: { list: vi.fn(), get: vi.fn(), create: vi.fn(), delete: vi.fn() },
      alertIntegrations: { list: vi.fn(), test: vi.fn() },
      alertRules: { list: vi.fn() },
      team: { list: vi.fn(), updateRole: vi.fn(), remove: vi.fn() },
      invitations: { create: vi.fn(), list: vi.fn(), revoke: vi.fn() },
      members: { list: vi.fn(), upsert: vi.fn(), remove: vi.fn() },
      health: { getVersion: vi.fn() },
      sessions: { stats: vi.fn() },
      transactions: {
        list: vi.fn(),
        get: vi.fn(),
        getStats: vi.fn(),
        getSpans: vi.fn(),
      },
      storage: {
        getSummary: vi.fn(),
        getProjects: vi.fn(),
        previewCleanup: vi.fn(),
        executeCleanup: vi.fn(),
        getCleanupStatus: vi.fn(),
        previewGcSourceMaps: vi.fn(),
        gcSourceMaps: vi.fn(),
      },
      spans: {
        list: vi.fn(),
      },
      agents: {
        getRuns: vi.fn(),
        getDuration: vi.fn(),
        getModelsByCalls: vi.fn(),
        getModelsByTokens: vi.fn(),
        getTools: vi.fn(),
        getTraces: vi.fn(),
      },
    };
    testEnv = await createTestEnv(mockClient);
  });

  afterEach(async () => {
    await testEnv.mcpClient.close();
  });

  // The version an MCP client sees comes from the handshake, so this is the
  // only place the advertised number is observable. Read package.json here
  // rather than importing the server's own constant: comparing the constant to
  // itself would pass no matter how stale it got.
  it('advertises the published package version', () => {
    const pkg = JSON.parse(
      readFileSync(new URL('../../package.json', import.meta.url), 'utf8'),
    ) as { version: string };

    const info = testEnv.mcpClient.getServerVersion();

    expect(info?.name).toBe('rustrak-mcp');
    expect(info?.version).toBe(pkg.version);
  });

  it(`registers all ${EXPECTED_TOOLS.length} tools`, async () => {
    const { tools } = await testEnv.mcpClient.listTools();
    const toolNames = tools.map((t: { name: string }) => t.name);

    for (const expected of EXPECTED_TOOLS) {
      expect(
        toolNames,
        `Expected tool "${expected}" to be registered`,
      ).toContain(expected);
    }

    expect(tools).toHaveLength(EXPECTED_TOOLS.length);
  });

  it('delete_issue has destructiveHint: true', async () => {
    const { tools } = await testEnv.mcpClient.listTools();
    const deleteIssue = tools.find(
      (t: { name: string }) => t.name === 'delete_issue',
    );
    expect(deleteIssue).toBeDefined();
    expect(deleteIssue?.annotations?.destructiveHint).toBe(true);
  });

  it('revoke_token has destructiveHint: true', async () => {
    const { tools } = await testEnv.mcpClient.listTools();
    const revokeToken = tools.find(
      (t: { name: string }) => t.name === 'revoke_token',
    );
    expect(revokeToken).toBeDefined();
    expect(revokeToken?.annotations?.destructiveHint).toBe(true);
  });

  it.each([
    'bulk_delete_issues',
    'remove_team_member',
    'revoke_invitation',
    'remove_project_member',
    'execute_storage_cleanup',
    'gc_storage_source_maps',
  ])('%s has destructiveHint: true', async (name) => {
    const { tools } = await testEnv.mcpClient.listTools();
    const tool = tools.find((t: { name: string }) => t.name === name);
    expect(tool).toBeDefined();
    expect(tool?.annotations?.destructiveHint).toBe(true);
  });
});
