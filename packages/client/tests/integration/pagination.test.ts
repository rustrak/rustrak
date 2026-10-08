import { HttpResponse, http } from 'msw/http';
import { beforeEach, describe, expect, it } from 'vitest';
import { RustrakClient } from '../../src/client.js';
import { expectOk } from '../helpers/result.js';
import { server } from '../setup.js';

describe('Pagination', () => {
  let client: RustrakClient;

  beforeEach(() => {
    client = new RustrakClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
    });
  });

  describe('Issues Pagination (Offset-based)', () => {
    it('should handle multiple pages', async () => {
      let requestCount = 0;

      server.use(
        http.get(
          'http://localhost:8080/api/projects/:projectId/issues',
          ({ request }) => {
            requestCount++;
            const url = new URL(request.url);
            const page = parseInt(url.searchParams.get('page') ?? '1', 10);

            if (page === 1) {
              return HttpResponse.json({
                items: [
                  {
                    id: '323e4567-e89b-12d3-a456-426614174000',
                    project_id: 1,
                    short_id: 'TEST-1',
                    title: 'Issue 1',
                    value: 'Issue 1 value',
                    first_seen: '2026-01-20T10:00:00.000Z',
                    last_seen: '2026-01-20T11:00:00.000Z',
                    event_count: 5,
                    level: 'error',
                    platform: 'javascript',
                    culprit: '',
                    logger: '',
                    status: 'unresolved',
                    substatus: 'new',
                    priority: 'high',
                    assigned_to: null,
                    assignee_type: null,
                    issue_type: 'error',
                    issue_category: 'error',
                    first_release: '',
                    last_release: '',
                    status_details: {},
                    user_report_count: 0,
                    is_resolved: false,
                    is_muted: false,
                  },
                ],
                total_count: 2,
                page: 1,
                per_page: 1,
                total_pages: 2,
              });
            } else if (page === 2) {
              return HttpResponse.json({
                items: [
                  {
                    id: '423e4567-e89b-12d3-a456-426614174000',
                    project_id: 1,
                    short_id: 'TEST-2',
                    title: 'Issue 2',
                    value: 'Issue 2 value',
                    first_seen: '2026-01-20T09:00:00.000Z',
                    last_seen: '2026-01-20T10:00:00.000Z',
                    event_count: 3,
                    level: 'error',
                    platform: 'javascript',
                    culprit: '',
                    logger: '',
                    status: 'unresolved',
                    substatus: 'new',
                    priority: 'high',
                    assigned_to: null,
                    assignee_type: null,
                    issue_type: 'error',
                    issue_category: 'error',
                    first_release: '',
                    last_release: '',
                    status_details: {},
                    user_report_count: 0,
                    is_resolved: false,
                    is_muted: false,
                  },
                ],
                total_count: 2,
                page: 2,
                per_page: 1,
                total_pages: 2,
              });
            }

            return HttpResponse.json({
              items: [],
              total_count: 2,
              page: page,
              per_page: 1,
              total_pages: 2,
            });
          },
        ),
      );

      // Get first page
      const firstPage = expectOk(await client.issues.list(1, { page: 1 }));
      expect(firstPage.items).toHaveLength(1);
      expect(firstPage.total_pages).toBe(2);

      // Get second page
      const secondPage = expectOk(await client.issues.list(1, { page: 2 }));
      expect(secondPage.items).toHaveLength(1);

      expect(requestCount).toBe(2);
    });

    it('should handle empty first page', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects/:projectId/issues', () => {
          return HttpResponse.json({
            items: [],
            total_count: 0,
            page: 1,
            per_page: 20,
            total_pages: 0,
          });
        }),
      );

      const response = expectOk(await client.issues.list(1));

      expect(response.items).toHaveLength(0);
      expect(response.total_count).toBe(0);
    });

    it('should handle pagination with query parameters', async () => {
      server.use(
        http.get(
          'http://localhost:8080/api/projects/:projectId/issues',
          ({ request }) => {
            const url = new URL(request.url);
            const sort = url.searchParams.get('sort');
            const order = url.searchParams.get('order');
            const filter = url.searchParams.get('filter');

            // Verify parameters are passed correctly
            expect(sort).toBe('last_seen');
            expect(order).toBe('asc');
            expect(filter).toBe('all');

            return HttpResponse.json({
              items: [],
              total_count: 0,
              page: 1,
              per_page: 20,
              total_pages: 0,
            });
          },
        ),
      );

      expectOk(
        await client.issues.list(1, {
          sort: 'last_seen',
          order: 'asc',
          filter: 'all',
        }),
      );
    });

    it('should preserve query params across pagination', async () => {
      const capturedParams: {
        first: URLSearchParams | null;
        second: URLSearchParams | null;
      } = {
        first: null,
        second: null,
      };

      server.use(
        http.get(
          'http://localhost:8080/api/projects/:projectId/issues',
          ({ request }) => {
            const url = new URL(request.url);
            const page = url.searchParams.get('page');

            if (!page || page === '1') {
              capturedParams.first = url.searchParams;
            } else {
              capturedParams.second = url.searchParams;
            }

            return HttpResponse.json({
              items: [],
              total_count: 0,
              page: parseInt(page ?? '1', 10),
              per_page: 20,
              total_pages: 2,
            });
          },
        ),
      );

      expectOk(
        await client.issues.list(1, {
          sort: 'last_seen',
          filter: 'all',
        }),
      );

      expectOk(
        await client.issues.list(1, {
          sort: 'last_seen',
          filter: 'all',
          page: 2,
        }),
      );

      expect(capturedParams.first?.get('sort')).toBe('last_seen');
      expect(capturedParams.second?.get('sort')).toBe('last_seen');
      expect(capturedParams.second?.get('page')).toBe('2');
    });
  });

  describe('Events Pagination (Cursor-based)', () => {
    it('should handle events pagination', async () => {
      server.use(
        http.get(
          'http://localhost:8080/api/projects/:projectId/issues/:issueId/events',
          ({ request }) => {
            const url = new URL(request.url);
            const cursor = url.searchParams.get('cursor');

            if (!cursor) {
              return HttpResponse.json({
                items: [
                  {
                    id: '523e4567-e89b-12d3-a456-426614174000',
                    event_id: '623e4567-e89b-12d3-a456-426614174000',
                    issue_id: '323e4567-e89b-12d3-a456-426614174000',
                    title: 'Event 1',
                    timestamp: '2026-01-20T11:00:00.000Z',
                    level: 'error',
                    platform: 'javascript',
                    release: '1.0.0',
                    environment: 'production',
                    event_type: 'error',
                  },
                ],
                next_cursor: 'next',
                has_more: true,
              });
            }

            return HttpResponse.json({
              items: [],
              has_more: false,
            });
          },
        ),
      );

      const firstPage = expectOk(
        await client.events.list(1, '323e4567-e89b-12d3-a456-426614174000'),
      );

      expect(firstPage.items).toHaveLength(1);
      expect(firstPage.has_more).toBe(true);

      const secondPage = expectOk(
        await client.events.list(1, '323e4567-e89b-12d3-a456-426614174000', {
          cursor: firstPage.next_cursor,
        }),
      );

      expect(secondPage.items).toHaveLength(0);
      expect(secondPage.has_more).toBe(false);
    });
  });

  describe('Edge Cases', () => {
    it('should handle per_page parameter', async () => {
      server.use(
        http.get(
          'http://localhost:8080/api/projects/:projectId/issues',
          ({ request }) => {
            const url = new URL(request.url);
            const perPage = url.searchParams.get('per_page');

            expect(perPage).toBe('5');

            return HttpResponse.json({
              items: [],
              total_count: 0,
              page: 1,
              per_page: 5,
              total_pages: 0,
            });
          },
        ),
      );

      expectOk(await client.issues.list(1, { per_page: 5 }));
    });

    it('should handle last page correctly', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects/:projectId/issues', () => {
          return HttpResponse.json({
            items: [],
            total_count: 50,
            page: 3,
            per_page: 20,
            total_pages: 3,
          });
        }),
      );

      const response = expectOk(await client.issues.list(1, { page: 3 }));

      expect(response.page).toBe(3);
      expect(response.total_pages).toBe(3);
    });

    it('should handle rapid pagination requests', async () => {
      server.use(
        http.get('http://localhost:8080/api/projects/:projectId/issues', () => {
          return HttpResponse.json({
            items: [],
            total_count: 0,
            page: 1,
            per_page: 20,
            total_pages: 0,
          });
        }),
      );

      // Make 10 rapid requests
      const requests = Array.from({ length: 10 }, (_, i) =>
        client.issues.list(1, { page: i + 1 }),
      );

      const responses = await Promise.all(requests);
      expect(responses).toHaveLength(10);
      responses.forEach((response) => {
        expect(expectOk(response).items).toBeDefined();
      });
    });
  });
});
