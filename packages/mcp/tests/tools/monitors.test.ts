import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createTestEnv, fail, ok } from '../setup.js';

const monitor = {
  id: '0b9b2c8e-1f3a-4c7d-8e6f-5a4b3c2d1e0f',
  slug: 'nightly-backup',
  name: 'Nightly backup',
  config: {
    schedule: { type: 'crontab', value: '0 3 * * *' },
    checkin_margin: 1,
    max_runtime: 30,
    timezone: 'UTC',
    failure_issue_threshold: 1,
    recovery_threshold: 1,
  },
  is_muted: false,
  created_at: '2026-01-01T00:00:00Z',
  environments: [],
};

describe('monitor tools', () => {
  let mockClient: any;
  let testEnv: Awaited<ReturnType<typeof createTestEnv>>;
  let callTool: Awaited<ReturnType<typeof createTestEnv>>['callTool'];

  beforeEach(async () => {
    mockClient = {
      monitors: {
        list: vi.fn(),
        get: vi.fn(),
        listCheckIns: vi.fn(),
        create: vi.fn(),
        update: vi.fn(),
        delete: vi.fn(),
      },
    };
    testEnv = await createTestEnv(mockClient);
    callTool = testEnv.callTool;
  });

  afterEach(async () => {
    await testEnv.mcpClient.close();
  });

  it('list_monitors returns the project monitors', async () => {
    mockClient.monitors.list.mockResolvedValue(ok([monitor]));

    const result = await callTool({
      name: 'list_monitors',
      arguments: { project_id: 1 },
    });

    expect(result.isError).toBeFalsy();
    expect(mockClient.monitors.list).toHaveBeenCalledWith(1);
    expect(JSON.parse(result.content[0].text)[0].slug).toBe('nightly-backup');
  });

  it('get_monitor reports a missing monitor as an error', async () => {
    mockClient.monitors.get.mockResolvedValue(
      fail({
        kind: 'not_found',
        status: 404,
        message: 'Resource not found: Monitor gone not found',
      }),
    );

    const result = await callTool({
      name: 'get_monitor',
      arguments: { project_id: 1, slug: 'gone' },
    });

    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toMatch(/not found/i);
  });

  it('list_monitor_checkins passes the filters through', async () => {
    mockClient.monitors.listCheckIns.mockResolvedValue(
      ok({ items: [], total_count: 0, page: 2, per_page: 10, total_pages: 0 }),
    );

    await callTool({
      name: 'list_monitor_checkins',
      arguments: {
        project_id: 1,
        slug: 'nightly-backup',
        page: 2,
        per_page: 10,
        environment: 'staging',
      },
    });

    expect(mockClient.monitors.listCheckIns).toHaveBeenCalledWith(
      1,
      'nightly-backup',
      { page: 2, per_page: 10, environment: 'staging' },
    );
  });

  it('create_monitor sends the schedule', async () => {
    mockClient.monitors.create.mockResolvedValue(ok(monitor));

    const result = await callTool({
      name: 'create_monitor',
      arguments: {
        project_id: 1,
        name: 'Nightly backup',
        config: {
          schedule: { type: 'crontab', value: '0 3 * * *' },
          timezone: 'Europe/Madrid',
        },
      },
    });

    expect(result.isError).toBeFalsy();
    expect(mockClient.monitors.create).toHaveBeenCalledWith(1, {
      name: 'Nightly backup',
      config: {
        schedule: { type: 'crontab', value: '0 3 * * *' },
        timezone: 'Europe/Madrid',
      },
    });
  });

  it('update_monitor mutes a monitor', async () => {
    mockClient.monitors.update.mockResolvedValue(
      ok({ ...monitor, is_muted: true }),
    );

    await callTool({
      name: 'update_monitor',
      arguments: { project_id: 1, slug: 'nightly-backup', is_muted: true },
    });

    expect(mockClient.monitors.update).toHaveBeenCalledWith(
      1,
      'nightly-backup',
      { is_muted: true },
    );
  });

  it('delete_monitor is marked destructive and confirms', async () => {
    mockClient.monitors.delete.mockResolvedValue(ok(undefined));

    const tools = await testEnv.mcpClient.listTools();
    const tool = tools.tools.find((t: any) => t.name === 'delete_monitor');
    expect(tool?.annotations?.destructiveHint).toBe(true);

    const result = await callTool({
      name: 'delete_monitor',
      arguments: { project_id: 1, slug: 'nightly-backup' },
    });

    expect(result.isError).toBeFalsy();
    expect(result.content[0]?.text).toMatch(/nightly-backup/);
  });
});
