import { HttpResponse, http } from 'msw/http';
import { beforeEach, describe, expect, it } from 'vitest';
import { RustrakClient } from '../../src/client.js';
import { expectErr, expectOk } from '../helpers/result.js';
import { server } from '../setup.js';

describe('AlertRulesResource Integration', () => {
  let client: RustrakClient;
  const projectId = 1;

  beforeEach(() => {
    client = new RustrakClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
    });
  });

  describe('list()', () => {
    it('should fetch all alert rules for a project', async () => {
      const rules = expectOk(await client.alertRules.list(projectId));

      expect(rules).toHaveLength(2);
      expect(rules[0]?.name).toBe('New Issue Alert');
      expect(rules[0]?.alert_type).toBe('new_issue');
      expect(rules[1]?.name).toBe('Regression Alert');
      expect(rules[1]?.alert_type).toBe('regression');
    });

    it('should validate response schema', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects/1/alert-rules', () => {
          return HttpResponse.json([
            {
              id: 1,
              project_id: 1,
              name: 'Invalid',
              alert_type: 'invalid_type', // Invalid alert type
              is_enabled: true,
              conditions: {},
              cooldown_minutes: 0,
              integration_ids: [],
              created_at: '2026-01-20T10:00:00.000Z',
              updated_at: '2026-01-20T10:00:00.000Z',
            },
          ]);
        }),
      );

      const result = await client.alertRules.list(projectId);

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_response');
    });

    it('should handle empty array', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects/1/alert-rules', () => {
          return HttpResponse.json([]);
        }),
      );

      const rules = expectOk(await client.alertRules.list(projectId));
      expect(rules).toHaveLength(0);
    });

    it('should return rules for specific project only', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects/2/alert-rules', () => {
          return HttpResponse.json([]);
        }),
      );

      const rules = expectOk(await client.alertRules.list(2));
      expect(rules).toHaveLength(0);
    });
  });

  describe('get()', () => {
    it('should fetch single rule by id', async () => {
      const rule = expectOk(await client.alertRules.get(projectId, 1));

      expect(rule.id).toBe(1);
      expect(rule.name).toBe('New Issue Alert');
      expect(rule.alert_type).toBe('new_issue');
      expect(rule.is_enabled).toBe(true);
      expect(rule.integration_ids).toEqual([1, 2]);
    });

    it('should report not_found for a non-existent rule', async () => {
      const result = await client.alertRules.get(projectId, 999);

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error.kind).toBe('not_found');
      expect(error.message).toBe(
        'Resource not found: Alert rule 999 not found',
      );
    });

    it('should validate datetime format', async () => {
      const rule = expectOk(await client.alertRules.get(projectId, 1));

      expect(new Date(rule.created_at).toISOString()).toBe(rule.created_at);
      expect(new Date(rule.updated_at).toISOString()).toBe(rule.updated_at);
    });

    it('should handle nullable last_triggered_at', async () => {
      const rule = expectOk(await client.alertRules.get(projectId, 2));

      expect(rule.last_triggered_at).toBeNull();
    });

    it('should include conditions object', async () => {
      const rule = expectOk(await client.alertRules.get(projectId, 1));

      expect(rule.conditions).toBeDefined();
      expect(typeof rule.conditions).toBe('object');
    });
  });

  describe('create()', () => {
    it('should create new_issue rule with channels', async () => {
      const rule = expectOk(
        await client.alertRules.create(projectId, {
          name: 'New Alert Rule',
          alert_type: 'new_issue',
          channels: [
            { integration_id: 1, routing_override: { channel: '#alerts' } },
          ],
        }),
      );

      expect(rule.name).toBe('New Alert Rule');
      expect(rule.alert_type).toBe('new_issue');
      expect(rule.id).toBe(3);
      expect(rule.is_enabled).toBe(true);
      expect(rule.integration_ids).toEqual([1]);
    });

    it('should create rule with multiple channels and routing overrides', async () => {
      const rule = expectOk(
        await client.alertRules.create(projectId, {
          name: 'Multi-channel Rule',
          alert_type: 'new_issue',
          channels: [
            { integration_id: 1, routing_override: {} },
            { integration_id: 2, routing_override: { channel: '#dev' } },
          ],
        }),
      );

      expect(rule.integration_ids).toEqual([1, 2]);
    });

    it('should create regression rule', async () => {
      const rule = expectOk(
        await client.alertRules.create(projectId, {
          name: 'Regression Rule',
          alert_type: 'regression',
          channels: [{ integration_id: 1, routing_override: {} }],
        }),
      );

      expect(rule.alert_type).toBe('regression');
    });

    it('should create unmute rule', async () => {
      const rule = expectOk(
        await client.alertRules.create(projectId, {
          name: 'Unmute Rule',
          alert_type: 'unmute',
          channels: [{ integration_id: 2, routing_override: {} }],
        }),
      );

      expect(rule.alert_type).toBe('unmute');
    });

    it('should create rule with cooldown', async () => {
      const rule = expectOk(
        await client.alertRules.create(projectId, {
          name: 'Cooldown Rule',
          alert_type: 'new_issue',
          channels: [{ integration_id: 1, routing_override: {} }],
          cooldown_minutes: 30,
        }),
      );

      expect(rule.cooldown_minutes).toBe(30);
    });

    it('should create rule with custom conditions', async () => {
      const rule = expectOk(
        await client.alertRules.create(projectId, {
          name: 'Conditional Rule',
          alert_type: 'new_issue',
          channels: [{ integration_id: 1, routing_override: {} }],
          conditions: { min_events: 5 },
        }),
      );

      expect(rule.conditions).toEqual({ min_events: 5 });
    });

    it('should create disabled rule', async () => {
      const rule = expectOk(
        await client.alertRules.create(projectId, {
          name: 'Disabled Rule',
          alert_type: 'new_issue',
          channels: [{ integration_id: 1, routing_override: {} }],
          is_enabled: false,
        }),
      );

      expect(rule.is_enabled).toBe(false);
    });

    it('should reject empty name', async () => {
      const result = await client.alertRules.create(projectId, {
        name: '',
        alert_type: 'new_issue',
        channels: [{ integration_id: 1, routing_override: {} }],
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should reject invalid alert type', async () => {
      const result = await client.alertRules.create(projectId, {
        name: 'Test',
        // @ts-expect-error - Testing runtime validation
        alert_type: 'invalid',
        channels: [],
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should accept empty channels array (relies on server validation)', async () => {
      const rule = expectOk(
        await client.alertRules.create(projectId, {
          name: 'Empty Channels Rule',
          alert_type: 'new_issue',
          channels: [],
        }),
      );

      expect(rule.name).toBe('Empty Channels Rule');
    });
  });

  describe('update()', () => {
    it('should update rule name', async () => {
      const updated = expectOk(
        await client.alertRules.update(projectId, 1, {
          name: 'Updated Rule Name',
        }),
      );

      expect(updated.name).toBe('Updated Rule Name');
      expect(updated.id).toBe(1);
    });

    it('should update rule enabled state', async () => {
      const updated = expectOk(
        await client.alertRules.update(projectId, 1, {
          is_enabled: false,
        }),
      );

      expect(updated.is_enabled).toBe(false);
    });

    it('should update channels with routing overrides', async () => {
      const updated = expectOk(
        await client.alertRules.update(projectId, 1, {
          channels: [
            { integration_id: 2, routing_override: { channel: '#dev' } },
          ],
        }),
      );

      expect(updated.integration_ids).toEqual([2]);
    });

    it('should update cooldown', async () => {
      const updated = expectOk(
        await client.alertRules.update(projectId, 1, {
          cooldown_minutes: 120,
        }),
      );

      expect(updated.cooldown_minutes).toBe(120);
    });

    it('should update conditions', async () => {
      const updated = expectOk(
        await client.alertRules.update(projectId, 1, {
          conditions: { min_events: 10 },
        }),
      );

      expect(updated.conditions).toEqual({ min_events: 10 });
    });

    it('should report not_found for a non-existent rule', async () => {
      const result = await client.alertRules.update(projectId, 999, {
        name: 'New Name',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('not_found');
    });

    it('should update timestamp', async () => {
      const original = expectOk(await client.alertRules.get(projectId, 1));
      const updated = expectOk(
        await client.alertRules.update(projectId, 1, {
          name: 'Updated',
        }),
      );

      expect(new Date(updated.updated_at).getTime()).toBeGreaterThanOrEqual(
        new Date(original.updated_at).getTime(),
      );
    });
  });

  describe('delete()', () => {
    it('should delete rule successfully', async () => {
      const result = await client.alertRules.delete(projectId, 1);

      expect(result.success).toBe(true);
      expect(expectOk(result)).toBeUndefined();
    });

    it('should report not_found for a non-existent rule', async () => {
      const result = await client.alertRules.delete(projectId, 999);

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('not_found');
    });
  });

  describe('listHistory()', () => {
    it('should fetch alert history for a project', async () => {
      const history = expectOk(await client.alertRules.listHistory(projectId));

      expect(history).toHaveLength(2);
      expect(history[0]?.alert_type).toBe('new_issue');
      expect(history[0]?.status).toBe('sent');
      expect(history[1]?.status).toBe('failed');
    });

    it('should validate response schema', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects/1/alert-history', () => {
          return HttpResponse.json([
            {
              id: 1,
              alert_type: 'new_issue',
              channel_type: 'webhook',
              channel_name: 'Test',
              status: 'invalid_status', // Invalid status
              attempt_count: 1,
              idempotency_key: 'key-1',
              created_at: '2026-01-20T10:00:00.000Z',
            },
          ]);
        }),
      );

      const result = await client.alertRules.listHistory(projectId);

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_response');
    });

    it('should handle empty history', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects/1/alert-history', () => {
          return HttpResponse.json([]);
        }),
      );

      const history = expectOk(await client.alertRules.listHistory(projectId));
      expect(history).toHaveLength(0);
    });

    it('should respect limit parameter', async () => {
      const history = expectOk(
        await client.alertRules.listHistory(projectId, {
          limit: 1,
        }),
      );

      expect(history).toHaveLength(1);
    });

    it('should include all required fields', async () => {
      const history = expectOk(await client.alertRules.listHistory(projectId));
      const item = history[0];

      expect(item).toBeDefined();
      expect(item?.id).toBeDefined();
      expect(item?.alert_type).toBeDefined();
      expect(item?.channel_type).toBeDefined();
      expect(item?.channel_name).toBeDefined();
      expect(item?.status).toBeDefined();
      expect(item?.attempt_count).toBeDefined();
      expect(item?.idempotency_key).toBeDefined();
      expect(item?.created_at).toBeDefined();
    });

    it('should include optional fields when present', async () => {
      const history = expectOk(await client.alertRules.listHistory(projectId));
      const sentItem = history.find((h) => h.status === 'sent');
      const failedItem = history.find((h) => h.status === 'failed');

      expect(sentItem?.sent_at).toBeDefined();
      expect(sentItem?.http_status_code).toBe(200);
      expect(failedItem?.error_message).toBe('Slack API timeout');
      expect(failedItem?.http_status_code).toBe(504);
    });

    it('should return integration_id in history entries', async () => {
      const history = expectOk(await client.alertRules.listHistory(projectId));

      expect(history[0]?.integration_id).toBe(1);
      expect(history[1]?.integration_id).toBe(2);
    });
  });
});
