import { HttpResponse, http } from 'msw/http';
import { beforeEach, describe, expect, it } from 'vitest';
import { RustrakClient } from '../../src/client.js';
import { expectErr, expectOk } from '../helpers/result.js';
import { appErrorResponse } from '../mocks/handlers.js';
import { server } from '../setup.js';

describe('AlertIntegrationsResource Integration', () => {
  let client: RustrakClient;

  beforeEach(() => {
    client = new RustrakClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
    });
  });

  describe('list()', () => {
    it('should fetch all alert integrations', async () => {
      const integrations = expectOk(await client.alertIntegrations.list());

      expect(integrations).toHaveLength(2);
      expect(integrations[0]?.name).toBe('Production Webhook');
      expect(integrations[0]?.provider_type).toBe('webhook');
      expect(integrations[1]?.name).toBe('Slack Alerts');
      expect(integrations[1]?.provider_type).toBe('slack');
    });

    it('should validate response schema', async () => {
      server.use(
        http.get('http://localhost:8080/api/integrations', () => {
          return HttpResponse.json([
            {
              id: 1,
              name: 'Invalid',
              provider_type: 'invalid_type', // Invalid provider type
              credentials: {},
              is_enabled: true,
              failure_count: 0,
              created_at: '2026-01-20T10:00:00.000Z',
              updated_at: '2026-01-20T10:00:00.000Z',
            },
          ]);
        }),
      );

      const result = await client.alertIntegrations.list();

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_response');
    });

    it('should handle empty array', async () => {
      server.use(
        http.get('http://localhost:8080/api/integrations', () => {
          return HttpResponse.json([]);
        }),
      );

      const integrations = expectOk(await client.alertIntegrations.list());
      expect(integrations).toHaveLength(0);
    });
  });

  describe('get()', () => {
    it('should fetch single integration by id', async () => {
      const integration = expectOk(await client.alertIntegrations.get(1));

      expect(integration.id).toBe(1);
      expect(integration.name).toBe('Production Webhook');
      expect(integration.provider_type).toBe('webhook');
      expect(integration.credentials).toEqual({
        url: 'https://example.com/webhook',
        secret: 'webhook-secret',
      });
    });

    it('should report not_found for a non-existent integration', async () => {
      const result = await client.alertIntegrations.get(999);

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error.kind).toBe('not_found');
      expect(error.message).toBe(
        'Resource not found: Integration 999 not found',
      );
    });

    it('should validate datetime format', async () => {
      const integration = expectOk(await client.alertIntegrations.get(1));

      expect(new Date(integration.created_at).toISOString()).toBe(
        integration.created_at,
      );
      expect(new Date(integration.updated_at).toISOString()).toBe(
        integration.updated_at,
      );
    });

    it('should handle nullable fields', async () => {
      const integration = expectOk(await client.alertIntegrations.get(1));

      expect(integration.last_failure_at).toBeNull();
      expect(integration.last_failure_message).toBeNull();
    });
  });

  describe('create()', () => {
    it('should create webhook integration', async () => {
      const integration = expectOk(
        await client.alertIntegrations.create({
          name: 'New Webhook',
          provider_type: 'webhook',
          credentials: {
            url: 'https://new.example.com/webhook',
          },
        }),
      );

      expect(integration.name).toBe('New Webhook');
      expect(integration.provider_type).toBe('webhook');
      expect(integration.id).toBe(3);
      expect(integration.is_enabled).toBe(true);
    });

    it('should create slack integration', async () => {
      const integration = expectOk(
        await client.alertIntegrations.create({
          name: 'Slack Bot',
          provider_type: 'slack',
          credentials: {
            method: 'bot_token',
            token: 'xoxb-test-token',
          },
        }),
      );

      expect(integration.provider_type).toBe('slack');
    });

    it('should create email integration', async () => {
      const integration = expectOk(
        await client.alertIntegrations.create({
          name: 'Email Alerts',
          provider_type: 'email',
          credentials: {
            smtp_host: 'smtp.example.com',
            smtp_port: 587,
            smtp_username: 'alerts@example.com',
            smtp_password: 'secret',
            from_address: 'alerts@example.com',
          },
        }),
      );

      expect(integration.provider_type).toBe('email');
    });

    it('should create custom webhook integration', async () => {
      const integration = expectOk(
        await client.alertIntegrations.create({
          name: 'Ops chat bridge',
          provider_type: 'custom_webhook',
          credentials: {
            url: 'https://example.com/hooks/incoming',
            template:
              '{"msgtype":"text","text":{"content":"{{ issue.title }}"}}',
          },
        }),
      );

      expect(integration.provider_type).toBe('custom_webhook');
    });

    it('should surface a rejected custom webhook create as an error', async () => {
      // The server owns credentials validation: an integration saved without
      // a template comes back 400, and the create promise resolves to the
      // error half of the Result rather than throwing.
      server.use(
        http.post('http://localhost:8080/api/integrations', () =>
          appErrorResponse(
            'ValidationError',
            'Validation error: Invalid custom webhook config: missing field `template`',
          ),
        ),
      );

      const result = await client.alertIntegrations.create({
        name: 'Broken Bridge',
        provider_type: 'custom_webhook',
        credentials: { url: 'https://example.com/hooks/incoming' },
      });

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error.kind).toBe('validation');
      expect(error.message).toContain('template');
    });

    it('should create integration with disabled state', async () => {
      const integration = expectOk(
        await client.alertIntegrations.create({
          name: 'Disabled Integration',
          provider_type: 'webhook',
          credentials: { url: 'https://example.com' },
          is_enabled: false,
        }),
      );

      expect(integration.is_enabled).toBe(false);
    });

    it('should reject empty name', async () => {
      const result = await client.alertIntegrations.create({
        name: '',
        provider_type: 'webhook',
        credentials: { url: 'https://example.com' },
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should reject invalid provider type', async () => {
      const result = await client.alertIntegrations.create({
        name: 'Test',
        // @ts-expect-error - Testing runtime validation
        provider_type: 'invalid',
        credentials: {},
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });
  });

  describe('update()', () => {
    it('should update integration name', async () => {
      const updated = expectOk(
        await client.alertIntegrations.update(1, {
          name: 'Updated Webhook Name',
        }),
      );

      expect(updated.name).toBe('Updated Webhook Name');
      expect(updated.id).toBe(1);
    });

    it('should update integration credentials', async () => {
      const updated = expectOk(
        await client.alertIntegrations.update(1, {
          credentials: {
            url: 'https://updated.example.com/webhook',
            secret: 'new-secret',
          },
        }),
      );

      expect(updated.credentials).toEqual({
        url: 'https://updated.example.com/webhook',
        secret: 'new-secret',
      });
    });

    it('should disable integration', async () => {
      const updated = expectOk(
        await client.alertIntegrations.update(1, {
          is_enabled: false,
        }),
      );

      expect(updated.is_enabled).toBe(false);
    });

    it('should report not_found for a non-existent integration', async () => {
      const result = await client.alertIntegrations.update(999, {
        name: 'New Name',
      });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('not_found');
    });

    it('should update timestamp', async () => {
      const original = expectOk(await client.alertIntegrations.get(1));
      const updated = expectOk(
        await client.alertIntegrations.update(1, {
          name: 'Updated',
        }),
      );

      expect(new Date(updated.updated_at).getTime()).toBeGreaterThanOrEqual(
        new Date(original.updated_at).getTime(),
      );
    });
  });

  describe('delete()', () => {
    it('should delete integration successfully', async () => {
      const result = await client.alertIntegrations.delete(1);

      expect(result.success).toBe(true);
      expect(expectOk(result)).toBeUndefined();
    });

    it('should report not_found for a non-existent integration', async () => {
      const result = await client.alertIntegrations.delete(999);

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('not_found');
    });
  });

  describe('test()', () => {
    it('should send test notification without routing override', async () => {
      const response = expectOk(await client.alertIntegrations.test(1));

      // `response.success` is the endpoint's own flag, not the Result
      // discriminant: unwrap first, then read it.
      expect(response.success).toBe(true);
      expect(response.message).toBe('Test notification sent successfully');
    });

    it('should send test notification with routing override', async () => {
      server.use(
        http.post(
          'http://localhost:8080/api/integrations/2/test',
          async ({ request }) => {
            const body = (await request.json()) as {
              routing_override?: Record<string, unknown>;
            };
            return HttpResponse.json({
              success: true,
              message: `Test sent to ${body.routing_override?.channel ?? 'unknown'}`,
            });
          },
        ),
      );

      const response = expectOk(
        await client.alertIntegrations.test(2, { channel: '#alerts' }),
      );
      expect(response.success).toBe(true);
      expect(response.message).toBe('Test sent to #alerts');
    });

    it('should report not_found for a non-existent integration', async () => {
      const result = await client.alertIntegrations.test(999);

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('not_found');
    });

    it('should handle test failure', async () => {
      server.use(
        http.post('http://localhost:8080/api/integrations/1/test', () => {
          return HttpResponse.json({
            success: false,
            message: 'Connection timeout',
          });
        }),
      );

      // A failed *test notification* is still a successful API call: the
      // endpoint reports the outcome in its body, not with a non-2xx.
      const response = expectOk(await client.alertIntegrations.test(1));
      expect(response.success).toBe(false);
      expect(response.message).toBe('Connection timeout');
    });
  });
});
