import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createTestEnv, fail, ok } from '../setup.js';

describe('schedule tools', () => {
  let mockClient: any;
  let testEnv: Awaited<ReturnType<typeof createTestEnv>>;
  let callTool: Awaited<ReturnType<typeof createTestEnv>>['callTool'];

  beforeEach(async () => {
    mockClient = { schedules: { preview: vi.fn() } };
    testEnv = await createTestEnv(mockClient);
    callTool = testEnv.callTool;
  });

  afterEach(async () => {
    await testEnv.mcpClient.close();
  });

  it('preview_schedule returns the next runs', async () => {
    mockClient.schedules.preview.mockResolvedValue(
      ok({ next_runs: ['2026-01-01T03:00:00Z'] }),
    );
    const result = await callTool({
      name: 'preview_schedule',
      arguments: {
        schedule: { type: 'crontab', value: '0 3 * * *' },
        timezone: 'Europe/Madrid',
      },
    });
    expect(mockClient.schedules.preview).toHaveBeenCalledWith({
      schedule: { type: 'crontab', value: '0 3 * * *' },
      timezone: 'Europe/Madrid',
    });
    expect(JSON.parse(result.content[0].text).next_runs).toHaveLength(1);
  });

  it('preview_schedule says why a schedule is refused', async () => {
    mockClient.schedules.preview.mockResolvedValue(
      fail({
        kind: 'validation',
        status: 400,
        message: 'Validation error: Invalid schedule: crontab: bad',
      }),
    );
    const result = await callTool({
      name: 'preview_schedule',
      arguments: { schedule: { type: 'crontab', value: 'nope' } },
    });
    expect(result.isError).toBe(true);
    expect(result.content[0]?.text).toMatch(/Invalid schedule/);
  });
});
