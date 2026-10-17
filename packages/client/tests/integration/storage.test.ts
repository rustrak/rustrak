import { HttpResponse, http } from 'msw/http';
import { describe, expect, it } from 'vitest';
import { RustrakClient } from '../../src/index.js';
import { expectErr, expectOk } from '../helpers/result.js';
import { server } from '../setup.js';

const client = new RustrakClient({
  baseUrl: 'http://localhost:8080',
  token: 'test-token',
});

describe('StorageResource', () => {
  describe('getSummary()', () => {
    it('returns the instance-wide storage summary', async () => {
      const summary = expectOk(await client.storage.getSummary());

      expect(summary.total_db_size_bytes).toBe(1048576);
      expect(summary.events_count).toBe(120);
      expect(summary.transactions_count).toBe(80);
      expect(summary.spans_count).toBe(640);
      expect(summary.logs_count).toBe(200);
      expect(summary.source_maps.total_bytes).toBe(650);
      expect(summary.source_maps.file_count).toBe(2);
    });
  });

  describe('getProjects()', () => {
    it('returns the per-project storage breakdown', async () => {
      const rows = expectOk(await client.storage.getProjects());

      expect(rows).toHaveLength(2);
      expect(rows[0].project_name).toBe('Test Project');
      expect(rows[0].events_count).toBe(100);
      expect(rows[0].logs_count).toBe(200);
      expect(rows[0].source_maps_count).toBe(2);
      expect(rows[1].events_count).toBe(0);
    });
  });

  describe('previewCleanup()', () => {
    it('returns the dry-run counts without deleting', async () => {
      const counts = expectOk(
        await client.storage.previewCleanup({
          older_than_days: 30,
        }),
      );

      expect(counts.events).toBe(20);
      expect(counts.transactions).toBe(10);
      expect(counts.spans).toBe(80);
      expect(counts.logs).toBe(50);
      expect(counts.issues_removed).toBe(3);
    });

    it('accepts a project scope', async () => {
      const counts = expectOk(
        await client.storage.previewCleanup({
          older_than_days: 30,
          project_id: 1,
        }),
      );
      expect(counts).toBeDefined();
    });

    it('rejects a non-positive retention window before sending a request', async () => {
      const preview = await client.storage.previewCleanup({
        older_than_days: 0,
      });
      expect(preview.success).toBe(false);
      expect(expectErr(preview).kind).toBe('invalid_request');

      const executed = await client.storage.executeCleanup({
        older_than_days: -1,
      });
      expect(executed.success).toBe(false);
      expect(expectErr(executed).kind).toBe('invalid_request');
    });
  });

  describe('executeCleanup()', () => {
    it('returns the started job', async () => {
      const status = expectOk(
        await client.storage.executeCleanup({
          older_than_days: 30,
        }),
      );

      expect(status.state).toBe('running');
      expect(status.removed.events).toBe(0);
      expect(status.finished_at).toBeNull();
    });

    it('forwards the data-type selection flags in the request body', async () => {
      let sentBody: Record<string, unknown> | undefined;
      server.use(
        http.post(
          'http://localhost:8080/api/storage/cleanup',
          async ({ request }) => {
            sentBody = (await request.json()) as Record<string, unknown>;
            return HttpResponse.json(
              {
                state: 'running',
                removed: {
                  events: 0,
                  transactions: 0,
                  spans: 0,
                  logs: 0,
                  issues_removed: 0,
                },
                started_at: '2026-10-07T10:00:00Z',
                finished_at: null,
                error: null,
              },
              { status: 202 },
            );
          },
        ),
      );

      expectOk(
        await client.storage.executeCleanup({
          older_than_days: 30,
          include_events: false,
          include_transactions: false,
          include_logs: true,
        }),
      );

      expect(sentBody).toMatchObject({
        older_than_days: 30,
        include_events: false,
        include_transactions: false,
        include_logs: true,
      });
    });

    it('reports a conflict while another cleanup is running', async () => {
      server.use(
        http.post('http://localhost:8080/api/storage/cleanup', () =>
          HttpResponse.json(
            {
              error: 'Conflict',
              message: 'Conflict: A storage cleanup is already running',
            },
            { status: 409 },
          ),
        ),
      );

      const result = await client.storage.executeCleanup({
        older_than_days: 30,
      });
      expect(expectErr(result).kind).toBe('conflict');
    });

    it('is never retried, so a lost response cannot start a second run', async () => {
      let calls = 0;
      server.use(
        http.post('http://localhost:8080/api/storage/cleanup', () => {
          calls += 1;
          return HttpResponse.json(
            { error: 'InternalError', message: 'boom' },
            { status: 503 },
          );
        }),
      );

      const result = await client.storage.executeCleanup({
        older_than_days: 30,
      });
      expect(result.success).toBe(false);
      expect(calls).toBe(1);
    });
  });

  describe('getCleanupStatus()', () => {
    it('returns the progress or outcome of the cleanup', async () => {
      const status = expectOk(await client.storage.getCleanupStatus());

      expect(status.state).toBe('completed');
      expect(status.removed.events).toBe(20);
      expect(status.removed.issues_removed).toBe(3);
      expect(status.finished_at).not.toBeNull();
    });
  });

  describe('previewGcSourceMaps()', () => {
    it('returns the orphaned files a GC would remove', async () => {
      const result = expectOk(await client.storage.previewGcSourceMaps());

      expect(result.files_removed).toBe(4);
      expect(result.bytes_freed).toBe(81920);
    });
  });

  describe('gcSourceMaps()', () => {
    it('returns the orphaned files removed and bytes freed', async () => {
      const result = expectOk(await client.storage.gcSourceMaps());

      expect(result.files_removed).toBe(4);
      expect(result.bytes_freed).toBe(81920);
    });
  });
});

describe('StorageResource auto-cleanup', () => {
  const URL = 'http://localhost:8080/api/storage/auto-cleanup';
  const settings = {
    enabled: true,
    schedule: { type: 'crontab', value: '0 3 * * *' },
    timezone: 'UTC',
    older_than_days: 30,
    include_events: false,
    include_transactions: true,
    include_logs: true,
  } as const;
  const client = new RustrakClient({
    baseUrl: 'http://localhost:8080',
    token: 'test-token',
  });

  it('reads the settings, the next run and the last run', async () => {
    server.use(
      http.get(URL, () =>
        HttpResponse.json({
          settings,
          next_run_at: '2026-01-02T03:00:00Z',
          last_run: {
            started_at: '2026-01-01T03:00:00Z',
            finished_at: '2026-01-01T03:00:04Z',
            status: 'ok',
            removed: {
              events: 0,
              transactions: 12,
              spans: 40,
              logs: 300,
              issues_removed: 0,
            },
            error: null,
          },
        }),
      ),
    );

    const auto = expectOk(await client.storage.getAutoCleanup());
    expect(auto.settings.include_events).toBe(false);
    expect(auto.last_run?.removed?.logs).toBe(300);
  });

  it('saves settings', async () => {
    let body: unknown;
    server.use(
      http.put(URL, async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({
          settings,
          next_run_at: null,
          last_run: null,
        });
      }),
    );

    expectOk(await client.storage.updateAutoCleanup(settings));
    expect(body).toEqual(settings);
  });

  it('refuses a retention under one day before sending it', async () => {
    const error = expectErr(
      await client.storage.updateAutoCleanup({
        ...settings,
        older_than_days: 0,
      }),
    );
    expect(error.kind).toBe('invalid_request');
  });

  it('reports a non-admin as forbidden', async () => {
    server.use(
      http.get(URL, () =>
        HttpResponse.json(
          {
            error: {
              type: 'Forbidden',
              message: 'Forbidden: Admin privileges required',
            },
          },
          { status: 403 },
        ),
      ),
    );
    expect(expectErr(await client.storage.getAutoCleanup()).kind).toBe(
      'forbidden',
    );
  });
});
