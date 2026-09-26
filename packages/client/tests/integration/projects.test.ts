import { HttpResponse, http } from 'msw';
import { beforeEach, describe, expect, it } from 'vitest';
import { RustrakClient } from '../../src/client.js';
import { expectErr, expectOk } from '../helpers/result.js';
import { server } from '../setup.js';

describe('ProjectsResource Integration', () => {
  let client: RustrakClient;

  beforeEach(() => {
    client = new RustrakClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
    });
  });

  describe('list()', () => {
    it('should fetch all projects', async () => {
      const response = expectOk(await client.projects.list());

      expect(response.items).toHaveLength(2);
      expect(response.items[0]?.name).toBe('Test Project');
      expect(response.items[1]?.name).toBe('Another Project');
      expect(response.total_count).toBe(2);
      expect(response.page).toBe(1);
    });

    it('should validate response schema', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return HttpResponse.json({
            items: [
              {
                id: 1,
                name: 'Invalid',
                slug: 'invalid',
                sentry_key: 'not-a-uuid', // Invalid UUID
                dsn: 'http://localhost:8080/1',
                stored_event_count: 0,
                digested_event_count: 0,
                rate_limited_event_count: 0,
                rate_limit_per_minute: null,
                rate_limit_per_hour: null,
                created_at: '2026-01-20T10:00:00.000Z',
                updated_at: '2026-01-20T10:00:00.000Z',
              },
            ],
            total_count: 1,
            page: 1,
            per_page: 20,
            total_pages: 1,
          });
        }),
      );

      const result = await client.projects.list();

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_response');
    });

    it('should handle empty array', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return HttpResponse.json({
            items: [],
            total_count: 0,
            page: 1,
            per_page: 20,
            total_pages: 0,
          });
        }),
      );

      // An empty collection is a success carrying `[]`, not a failure.
      const response = expectOk(await client.projects.list());
      expect(response.items).toHaveLength(0);
    });

    it('should omit stats_period from the query when not requested', async () => {
      let requestUrl: string | undefined;
      server.use(
        http.get('http://localhost:8080/api/projects', ({ request }) => {
          requestUrl = request.url;
          return HttpResponse.json({
            items: [],
            total_count: 0,
            page: 1,
            per_page: 20,
            total_pages: 0,
          });
        }),
      );

      expectOk(await client.projects.list());

      expect(requestUrl).not.toContain('stats_period');
    });

    it('should forward stats_period as a query param', async () => {
      let requestUrl: string | undefined;
      server.use(
        http.get('http://localhost:8080/api/projects', ({ request }) => {
          requestUrl = request.url;
          return HttpResponse.json({
            items: [],
            total_count: 0,
            page: 1,
            per_page: 20,
            total_pages: 0,
          });
        }),
      );

      expectOk(await client.projects.list({ stats_period: '24h' }));

      expect(requestUrl).toContain('stats_period=24h');
    });

    it('should parse per-row stats when the server attaches them', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return HttpResponse.json({
            items: [
              {
                id: 1,
                name: 'With Stats',
                slug: 'with-stats',
                sentry_key: '550e8400-e29b-41d4-a716-446655440000',
                dsn: 'http://key@localhost:8080/1',
                stored_event_count: 0,
                digested_event_count: 0,
                rate_limited_event_count: 0,
                rate_limit_per_minute: null,
                rate_limit_per_hour: null,
                created_at: '2026-01-20T10:00:00.000Z',
                updated_at: '2026-01-20T10:00:00.000Z',
                platform: 'node',
                stats: {
                  trend: [0, 1, 4, 2],
                  events: { current: 7, previous: 3 },
                  new_issues: { current: 2, previous: 1 },
                  open_issues: 2,
                  fatal_issues: 1,
                },
              },
            ],
            total_count: 1,
            page: 1,
            per_page: 20,
            total_pages: 1,
          });
        }),
      );

      const response = expectOk(
        await client.projects.list({ stats_period: '24h' }),
      );

      expect(response.items[0]?.stats?.events.current).toBe(7);
      expect(response.items[0]?.stats?.trend).toEqual([0, 1, 4, 2]);
      expect(response.items[0]?.stats?.fatal_issues).toBe(1);
    });

    /**
     * All-time requests have no earlier window to compare against, so the
     * server sends null rather than a zero that would render as "+100%".
     */
    it('should accept a null previous count', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects', () => {
          return HttpResponse.json({
            items: [
              {
                id: 1,
                name: 'All Time',
                slug: 'all-time',
                sentry_key: '550e8400-e29b-41d4-a716-446655440000',
                dsn: 'http://key@localhost:8080/1',
                stored_event_count: 0,
                digested_event_count: 0,
                rate_limited_event_count: 0,
                rate_limit_per_minute: null,
                rate_limit_per_hour: null,
                created_at: '2026-01-20T10:00:00.000Z',
                updated_at: '2026-01-20T10:00:00.000Z',
                platform: null,
                stats: {
                  trend: [],
                  events: { current: 9, previous: null },
                  new_issues: { current: 0, previous: null },
                  open_issues: 0,
                  fatal_issues: 0,
                },
              },
            ],
            total_count: 1,
            page: 1,
            per_page: 20,
            total_pages: 1,
          });
        }),
      );

      const response = expectOk(await client.projects.list());

      expect(response.items[0]?.stats?.events.previous).toBeNull();
    });
  });

  describe('rateLimits()', () => {
    it('reads the server per-project limits', async () => {
      server.use(
        http.get('http://localhost:8080/api/rate-limits', () =>
          HttpResponse.json({
            project_per_minute: 20000,
            project_per_hour: 300000,
          }),
        ),
      );

      expect(expectOk(await client.projects.rateLimits())).toEqual({
        project_per_minute: 20000,
        project_per_hour: 300000,
      });
    });

    it('reports an unauthenticated caller as such', async () => {
      server.use(
        http.get('http://localhost:8080/api/rate-limits', () =>
          HttpResponse.json(
            {
              error: {
                type: 'Unauthorized',
                message: 'Authentication required',
              },
            },
            { status: 401 },
          ),
        ),
      );

      expect(expectErr(await client.projects.rateLimits()).kind).toBe(
        'unauthenticated',
      );
    });
  });

  describe('get()', () => {
    it('should fetch single project by id', async () => {
      const project = expectOk(await client.projects.get(1));

      expect(project.id).toBe(1);
      expect(project.name).toBe('Test Project');
      expect(project.slug).toBe('test-project');
    });

    it('should report not_found for non-existent project', async () => {
      const result = await client.projects.get(999);

      expect(result.success).toBe(false);
      const error = expectErr(result);
      expect(error.kind).toBe('not_found');
      expect(error.message).toBe(
        'Resource not found: Project with id 999 not found',
      );
    });

    it('should validate UUID format', async () => {
      const project = expectOk(await client.projects.get(1));

      expect(project.sentry_key).toMatch(
        /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i,
      );
    });

    it('should validate datetime format', async () => {
      const project = expectOk(await client.projects.get(1));

      expect(new Date(project.created_at).toISOString()).toBe(
        project.created_at,
      );
      expect(new Date(project.updated_at).toISOString()).toBe(
        project.updated_at,
      );
    });
  });

  describe('create()', () => {
    it('should create project with all fields', async () => {
      const project = expectOk(
        await client.projects.create({
          name: 'New Project',
          slug: 'new-project',
        }),
      );

      expect(project.name).toBe('New Project');
      expect(project.slug).toBe('new-project');
      expect(project.id).toBe(3);
    });

    it('should create project without optional slug', async () => {
      const project = expectOk(
        await client.projects.create({
          name: 'Auto Slug Project',
        }),
      );

      expect(project.name).toBe('Auto Slug Project');
      expect(project.slug).toBeTruthy();
    });

    it('should create project with a platform', async () => {
      const project = expectOk(
        await client.projects.create({
          name: 'Platform Project',
          platform: 'javascript-nextjs',
        }),
      );

      expect(project.platform).toBe('javascript-nextjs');
    });

    it('should reject an empty platform', async () => {
      const result = await client.projects.create({
        name: 'Empty Platform',
        platform: '',
      });

      expect(result.success).toBe(false);
      // Caller input, checked before anything reaches the network:
      // `invalid_request`, not the `invalid_response` a bad server body gets.
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should leave platform null when omitted', async () => {
      const project = expectOk(
        await client.projects.create({
          name: 'No Platform Project',
        }),
      );

      expect(project.platform).toBeNull();
    });

    it('should reject empty name', async () => {
      const result = await client.projects.create({ name: '' });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should reject malformed input', async () => {
      const result = await client.projects.create(
        // @ts-expect-error - Testing runtime validation
        { invalid: 'field' },
      );

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });
  });

  describe('update()', () => {
    it('should update project name', async () => {
      const updated = expectOk(
        await client.projects.update(1, {
          name: 'Updated Name',
        }),
      );

      expect(updated.name).toBe('Updated Name');
      expect(updated.id).toBe(1);
    });

    it('should report not_found for non-existent project', async () => {
      const result = await client.projects.update(999, { name: 'New Name' });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('not_found');
    });

    it('should reject empty name', async () => {
      const result = await client.projects.update(1, { name: '' });

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('invalid_request');
    });

    it('should update timestamp', async () => {
      const original = expectOk(await client.projects.get(1));
      const updated = expectOk(
        await client.projects.update(1, { name: 'Updated' }),
      );

      expect(new Date(updated.updated_at).getTime()).toBeGreaterThanOrEqual(
        new Date(original.updated_at).getTime(),
      );
    });

    it('should update project platform', async () => {
      const updated = expectOk(
        await client.projects.update(1, {
          platform: 'python',
        }),
      );

      expect(updated.platform).toBe('python');
    });

    it('should update project slug', async () => {
      const updated = expectOk(
        await client.projects.update(1, {
          slug: 'renamed-slug',
        }),
      );

      expect(updated.slug).toBe('renamed-slug');
    });
  });

  describe('delete()', () => {
    it('should delete project successfully', async () => {
      const result = await client.projects.delete(1);

      expect(result.success).toBe(true);
      expect(expectOk(result)).toBeUndefined();
    });

    it('should report not_found for non-existent project', async () => {
      const result = await client.projects.delete(999);

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('not_found');
    });
  });
});
