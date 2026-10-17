import { HttpResponse, http } from 'msw/http';
import { beforeEach, describe, expect, it } from 'vitest';
import { RustrakClient } from '../../src/client.js';
import { expectErr, expectOk } from '../helpers/result.js';
import { server } from '../setup.js';

const BASE = 'http://localhost:8080/api/projects/1/monitors';

const checkIn = {
  id: '4f8c1d1e-8a6b-4a1e-9b5c-0d2a3b4c5d6e',
  environment: 'production',
  status: 'ok',
  duration_ms: 1200,
  date_added: '2026-01-01T05:00:00Z',
  date_updated: '2026-01-01T05:00:01Z',
  expected_time: '2026-01-01T05:00:00Z',
};

const monitor = {
  id: '0b9b2c8e-1f3a-4c7d-8e6f-5a4b3c2d1e0f',
  slug: 'nightly-backup',
  name: 'Nightly backup',
  config: {
    schedule: { type: 'crontab', value: '0 3 * * *' },
    checkin_margin: 1,
    max_runtime: 30,
    timezone: 'Europe/Madrid',
    failure_issue_threshold: 1,
    recovery_threshold: 1,
  },
  is_muted: false,
  created_at: '2026-01-01T00:00:00Z',
  environments: [
    {
      id: '7c6b5a49-3827-4615-a4b3-c2d1e0f9a8b7',
      environment: 'production',
      status: 'ok',
      next_checkin: '2026-01-02T02:00:00Z',
      next_checkin_latest: '2026-01-02T02:01:00Z',
      last_checkin: '2026-01-01T02:00:00Z',
      recent_checkins: [checkIn],
    },
  ],
};

describe('MonitorsResource', () => {
  let client: RustrakClient;

  beforeEach(() => {
    client = new RustrakClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
    });
  });

  it('lists monitors with their environments', async () => {
    server.use(http.get(BASE, () => HttpResponse.json([monitor])));

    const monitors = expectOk(await client.monitors.list(1));

    expect(monitors[0]?.config.schedule).toEqual({
      type: 'crontab',
      value: '0 3 * * *',
    });
    expect(monitors[0]?.environments[0]?.recent_checkins[0]?.status).toBe('ok');
  });

  it('parses an interval schedule', async () => {
    server.use(
      http.get(`${BASE}/hourly`, () =>
        HttpResponse.json({
          ...monitor,
          config: {
            ...monitor.config,
            schedule: { type: 'interval', value: 2, unit: 'hour' },
          },
        }),
      ),
    );

    const got = expectOk(await client.monitors.get(1, 'hourly'));

    expect(got.config.schedule).toEqual({
      type: 'interval',
      value: 2,
      unit: 'hour',
    });
  });

  it('rejects a response with an unknown check-in status', async () => {
    server.use(
      http.get(BASE, () =>
        HttpResponse.json([
          {
            ...monitor,
            environments: [
              {
                ...monitor.environments[0],
                recent_checkins: [{ ...checkIn, status: 'exploded' }],
              },
            ],
          },
        ]),
      ),
    );

    expect(expectErr(await client.monitors.list(1)).kind).toBe(
      'invalid_response',
    );
  });

  it('creates a monitor', async () => {
    let body: unknown;
    server.use(
      http.post(BASE, async ({ request }) => {
        body = await request.json();
        return HttpResponse.json(monitor, { status: 201 });
      }),
    );

    expectOk(
      await client.monitors.create(1, {
        name: 'Nightly backup',
        config: { schedule: { type: 'crontab', value: '0 3 * * *' } },
      }),
    );

    expect(body).toEqual({
      name: 'Nightly backup',
      config: { schedule: { type: 'crontab', value: '0 3 * * *' } },
    });
  });

  it('refuses to send a create without a name', async () => {
    const result = await client.monitors.create(1, {
      name: '',
      config: { schedule: { type: 'crontab', value: '0 3 * * *' } },
    });

    expect(expectErr(result).kind).toBe('invalid_request');
  });

  it('surfaces a taken slug as a conflict', async () => {
    server.use(
      http.post(BASE, () =>
        HttpResponse.json(
          {
            type: 'Conflict',
            message:
              'Conflict: A monitor with slug nightly-backup already exists',
            fields: [{ field: 'slug', code: 'already_exists' }],
          },
          { status: 409 },
        ),
      ),
    );

    const error = expectErr(
      await client.monitors.create(1, {
        name: 'Nightly backup',
        config: { schedule: { type: 'crontab', value: '0 3 * * *' } },
      }),
    );

    expect(error.kind).toBe('conflict');
  });

  it('updates a monitor', async () => {
    server.use(
      http.put(`${BASE}/nightly-backup`, async ({ request }) => {
        const patch = (await request.json()) as { is_muted: boolean };
        return HttpResponse.json({ ...monitor, is_muted: patch.is_muted });
      }),
    );

    const updated = expectOk(
      await client.monitors.update(1, 'nightly-backup', { is_muted: true }),
    );

    expect(updated.is_muted).toBe(true);
  });

  it('deletes a monitor', async () => {
    server.use(
      http.delete(
        `${BASE}/nightly-backup`,
        () => new HttpResponse(null, { status: 204 }),
      ),
    );

    expectOk(await client.monitors.delete(1, 'nightly-backup'));
  });

  it('reports a missing monitor as not_found', async () => {
    server.use(
      http.get(`${BASE}/gone`, () =>
        HttpResponse.json(
          {
            type: 'NotFound',
            message: 'Resource not found: Monitor gone not found',
          },
          { status: 404 },
        ),
      ),
    );

    expect(expectErr(await client.monitors.get(1, 'gone')).kind).toBe(
      'not_found',
    );
  });

  it('lists check-ins with filters', async () => {
    let query = '';
    server.use(
      http.get(`${BASE}/nightly-backup/checkins`, ({ request }) => {
        query = new URL(request.url).search;
        return HttpResponse.json({
          items: [checkIn, { ...checkIn, status: 'missed', duration_ms: null }],
          total_count: 2,
          page: 2,
          per_page: 2,
          total_pages: 1,
        });
      }),
    );

    const page = expectOk(
      await client.monitors.listCheckIns(1, 'nightly-backup', {
        page: 2,
        per_page: 2,
        environment: 'staging',
      }),
    );

    expect(query).toBe('?page=2&per_page=2&environment=staging');
    expect(page.items[1]?.status).toBe('missed');
  });
});
