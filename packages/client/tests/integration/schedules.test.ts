import { HttpResponse, http } from 'msw/http';
import { beforeEach, describe, expect, it } from 'vitest';
import { RustrakClient } from '../../src/client.js';
import { expectErr, expectOk } from '../helpers/result.js';
import { server } from '../setup.js';

const URL = 'http://localhost:8080/api/schedules/preview';

describe('SchedulesResource', () => {
  let client: RustrakClient;

  beforeEach(() => {
    client = new RustrakClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
    });
  });

  it('previews the next runs of a schedule in a zone', async () => {
    let body: unknown;
    server.use(
      http.post(URL, async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({
          next_runs: ['2026-01-01T06:00:00Z', '2026-01-01T07:00:00Z'],
        });
      }),
    );

    const preview = expectOk(
      await client.schedules.preview({
        schedule: { type: 'crontab', value: '0 * * * *' },
        timezone: 'Europe/Madrid',
      }),
    );

    expect(body).toEqual({
      schedule: { type: 'crontab', value: '0 * * * *' },
      timezone: 'Europe/Madrid',
    });
    expect(preview.next_runs).toHaveLength(2);
  });

  it('refuses a zero interval before sending it', async () => {
    const error = expectErr(
      await client.schedules.preview({
        schedule: { type: 'interval', value: 0, unit: 'hour' },
      }),
    );
    expect(error.kind).toBe('invalid_request');
  });

  it('reports a schedule the server refuses as a validation error', async () => {
    server.use(
      http.post(URL, () =>
        HttpResponse.json(
          {
            error: {
              type: 'ValidationError',
              message: 'Validation error: Invalid schedule: crontab: bad',
              fields: [{ field: 'schedule', code: 'invalid' }],
            },
          },
          { status: 400 },
        ),
      ),
    );

    const error = expectErr(
      await client.schedules.preview({
        schedule: { type: 'crontab', value: 'nope' },
      }),
    );
    expect(error.kind).toBe('validation');
  });
});
