import { HttpResponse, http } from 'msw/http';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { RustrakClient } from '../../src/client.js';
import { expectErr, expectOk } from '../helpers/result.js';
import { appErrorResponse } from '../mocks/handlers.js';
import { server } from '../setup.js';

describe('IssuesResource Integration', () => {
  let client: RustrakClient;

  beforeEach(() => {
    client = new RustrakClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
    });
  });

  describe('list()', () => {
    it('should fetch issues with pagination', async () => {
      const response = expectOk(await client.issues.list(1));

      expect(response.items).toHaveLength(2);
      expect(response.total_count).toBe(2);
      expect(response.page).toBe(1);
      expect(response.per_page).toBe(20);
      expect(response.total_pages).toBe(1);
    });

    it('should support page pagination', async () => {
      const firstPage = expectOk(await client.issues.list(1));
      const secondPage = expectOk(await client.issues.list(1, { page: 2 }));

      expect(firstPage.items).toHaveLength(2);
      expect(secondPage.items).toHaveLength(0);
      expect(secondPage.page).toBe(2);
    });

    it('should support sort parameter', async () => {
      const response = expectOk(
        await client.issues.list(1, {
          sort: 'last_seen',
        }),
      );

      expect(response.items).toBeDefined();
    });

    it('should support order parameter', async () => {
      const response = expectOk(
        await client.issues.list(1, {
          order: 'asc',
        }),
      );

      expect(response.items).toBeDefined();
    });

    it('should support filter parameter', async () => {
      const response = expectOk(
        await client.issues.list(1, {
          filter: 'all',
        }),
      );

      expect(response.items).toBeDefined();
    });

    it('should handle empty results', async () => {
      const response = expectOk(await client.issues.list(1, { page: 99 }));

      expect(response.items).toHaveLength(0);
    });

    it('should validate UUID format in response', async () => {
      const response = expectOk(await client.issues.list(1));

      response.items.forEach((issue) => {
        expect(issue.id).toMatch(
          /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i,
        );
      });
    });

    it('should handle null level and platform', async () => {
      const response = expectOk(await client.issues.list(1));
      const firstIssue = response.items[0];

      expect(firstIssue).toBeDefined();
      // level and platform can be string or null
      expect(['string', 'object']).toContain(typeof firstIssue!.level);
    });
  });

  describe('get()', () => {
    it('should fetch single issue', async () => {
      const issue = expectOk(
        await client.issues.get(1, '323e4567-e89b-12d3-a456-426614174000'),
      );

      expect(issue.id).toBe('323e4567-e89b-12d3-a456-426614174000');
      expect(issue.title).toBe('TypeError: Cannot read property');
    });

    it('should report not_found for a non-existent issue', async () => {
      const result = await client.issues.get(
        1,
        '999e4567-e89b-12d3-a456-426614174000',
      );

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('not_found');
    });
  });

  describe('updateState()', () => {
    it('should resolve issue', async () => {
      const updated = expectOk(
        await client.issues.updateState(
          1,
          '323e4567-e89b-12d3-a456-426614174000',
          { is_resolved: true },
        ),
      );

      expect(updated.is_resolved).toBe(true);
    });

    it('should mute issue', async () => {
      const updated = expectOk(
        await client.issues.updateState(
          1,
          '323e4567-e89b-12d3-a456-426614174000',
          { is_muted: true },
        ),
      );

      expect(updated.is_muted).toBe(true);
    });

    it('should let is_resolved win over is_muted (unified status model)', async () => {
      // Under the #165 status model, status is single-valued: resolving an
      // issue clears the muted/ignored state (they are mutually exclusive).
      const updated = expectOk(
        await client.issues.updateState(
          1,
          '323e4567-e89b-12d3-a456-426614174000',
          {
            is_resolved: true,
            is_muted: true,
          },
        ),
      );

      expect(updated.is_resolved).toBe(true);
      expect(updated.is_muted).toBe(false);
      expect(updated.status).toBe('resolved');
    });

    it('should report not_found for a non-existent issue', async () => {
      const result = await client.issues.updateState(
        1,
        '999e4567-e89b-12d3-a456-426614174000',
        { is_resolved: true },
      );

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('not_found');
    });
  });

  describe('delete()', () => {
    it('should delete issue successfully', async () => {
      const result = await client.issues.delete(
        1,
        '323e4567-e89b-12d3-a456-426614174000',
      );

      expect(result.success).toBe(true);
      expect(expectOk(result)).toBeUndefined();
    });

    it('should report not_found for a non-existent issue', async () => {
      const result = await client.issues.delete(
        1,
        '999e4567-e89b-12d3-a456-426614174000',
      );

      expect(result.success).toBe(false);
      expect(expectErr(result).kind).toBe('not_found');
    });
  });

  describe('status model fields (#165)', () => {
    it('should expose status, substatus, priority, culprit and metadata', async () => {
      const issue = expectOk(
        await client.issues.get(1, '323e4567-e89b-12d3-a456-426614174000'),
      );

      expect(issue.status).toBe('unresolved');
      expect(issue.substatus).toBe('new');
      expect(issue.priority).toBe('high');
      expect(issue.culprit).toBe('handleRequest');
      expect(issue.issue_type).toBe('error');
      expect(issue.issue_category).toBe('error');
      expect(issue.user_report_count).toBe(0);
      expect(issue.status_details).toEqual({});
      // legacy derived fields remain for backward compatibility
      expect(issue.is_resolved).toBe(false);
      expect(issue.is_muted).toBe(false);
    });

    it('should update issue status with the canonical field', async () => {
      const issue = expectOk(
        await client.issues.updateState(
          1,
          '323e4567-e89b-12d3-a456-426614174000',
          { status: 'resolved' },
        ),
      );
      expect(issue.status).toBe('resolved');
    });

    it('should default user_report_count to 0 when the list omits it', async () => {
      // The list endpoint returns the lean IssueResponse (no user_report_count
      // enrichment); the schema must tolerate its absence, not reject the page.
      const response = expectOk(await client.issues.list(1));
      expect(response.items.length).toBeGreaterThan(0);
      for (const issue of response.items) {
        expect(issue.user_report_count).toBe(0);
      }
    });
  });

  describe('search (#165)', () => {
    it('should filter issues by free-text query', async () => {
      const response = expectOk(
        await client.issues.list(1, { q: 'ReferenceError' }),
      );
      expect(response.items).toHaveLength(1);
      expect(response.items[0]!.title).toContain('ReferenceError');
    });
  });

  describe('hashes / tags / aggregates / stats (#165)', () => {
    const issueId = '323e4567-e89b-12d3-a456-426614174000';

    it('should list grouping hashes', async () => {
      const hashes = expectOk(await client.issues.getHashes(1, issueId));
      expect(hashes).toHaveLength(1);
      expect(hashes[0]!.grouping_key_hash).toHaveLength(64);
    });

    it('should list tag values for a key as a bare list', async () => {
      const result = expectOk(
        await client.issues.getTagValues(1, issueId, 'browser'),
      );
      expect(Array.isArray(result)).toBe(true);
      expect(result[0]).toMatchObject({
        key: 'browser',
        name: 'browser',
        value: 'chrome',
        count: 2,
      });
      expect(typeof result[0]!.first_seen).toBe('string');
      expect(typeof result[0]!.last_seen).toBe('string');
    });

    it('should fetch aggregates (user count + top tags)', async () => {
      const agg = expectOk(await client.issues.getAggregates(1, issueId));
      expect(agg.user_count).toBe(2);
      expect(agg.tags[0]!.key).toBe('browser');
    });

    it('should fetch a stats timeseries', async () => {
      const stats = expectOk(await client.issues.getStats(1, issueId, '24h'));
      expect(stats.data).toHaveLength(2);
      expect(stats.data[0]).toEqual([1000, 3]);
    });
  });

  describe('activity, comments & social (#165)', () => {
    const issueId = '323e4567-e89b-12d3-a456-426614174000';

    it('should list activity', async () => {
      const activity = expectOk(await client.issues.getActivity(1, issueId));
      expect(activity).toHaveLength(1);
      expect(activity[0]!.type).toBe('note');
    });

    it('should add a comment', async () => {
      const entry = expectOk(
        await client.issues.addComment(1, issueId, {
          text: 'hello world',
        }),
      );
      expect(entry.type).toBe('note');
      expect(entry.data).toContain('hello world');
    });

    it('should toggle bookmark', async () => {
      const res = expectOk(await client.issues.setBookmark(1, issueId, true));
      expect(res.is_bookmarked).toBe(true);
    });

    it('should toggle subscription', async () => {
      const res = expectOk(
        await client.issues.setSubscription(1, issueId, false),
      );
      expect(res.is_subscribed).toBe(false);
    });

    it('should mark seen', async () => {
      const res = expectOk(await client.issues.markSeen(1, issueId));
      expect(res.has_seen).toBe(true);
    });

    it('should list and create user reports', async () => {
      const reports = expectOk(await client.issues.listUserReports(1, issueId));
      expect(reports).toHaveLength(1);
      expect(reports[0]!.name).toBe('Jane');

      const created = expectOk(
        await client.issues.createUserReport(1, issueId, {
          name: 'Bob',
          email: 'bob@example.com',
          comments: 'broke too',
        }),
      );
      expect(created.name).toBe('Bob');
    });
  });

  describe('bulk operations & deploys (#165)', () => {
    const ids = [
      '323e4567-e89b-12d3-a456-426614174000',
      '423e4567-e89b-12d3-a456-426614174000',
    ];

    it('should bulk-update issues', async () => {
      const res = expectOk(
        await client.issues.bulkUpdate(1, {
          ids,
          status: 'resolved',
        }),
      );
      expect(res.updated).toBe(2);
    });

    it('should bulk-delete issues', async () => {
      const res = expectOk(await client.issues.bulkDelete(1, { ids }));
      expect(res.deleted).toBe(2);
    });

    it.each(['open', 'resolved', 'muted', 'all'] as const)(
      'should send the %s filter in the DELETE body',
      async (filter) => {
        const requestBody = vi.fn();
        server.use(
          http.delete(
            'http://localhost:8080/api/projects/1/issues',
            async ({ request }) => {
              requestBody(await request.json());
              return HttpResponse.json({ deleted: 3 });
            },
          ),
        );
        const result = expectOk(await client.issues.bulkDelete(1, { filter }));
        expect(result.deleted).toBe(3);
        expect(requestBody).toHaveBeenCalledExactlyOnceWith({ filter });
      },
    );

    it('should reject conflicting selectors before making a request', async () => {
      const request = vi.fn(() => HttpResponse.json({ deleted: 0 }));
      server.use(
        http.delete('http://localhost:8080/api/projects/1/issues', request),
      );
      const error = expectErr(
        await client.issues.bulkDelete(1, { ids, filter: 'all' }),
      );
      expect(error.kind).toBe('invalid_request');
      expect(request).not.toHaveBeenCalled();
    });

    it('should return permission errors when deleting by filter', async () => {
      server.use(
        http.delete('http://localhost:8080/api/projects/1/issues', () =>
          appErrorResponse('Forbidden', 'Forbidden: Insufficient project role'),
        ),
      );
      const error = expectErr(
        await client.issues.bulkDelete(1, { filter: 'all' }),
      );
      expect(error).toMatchObject({
        kind: 'forbidden',
        status: 403,
        message: 'Forbidden: Insufficient project role',
      });
    });

    it('should resolve in next release', async () => {
      const issue = expectOk(
        await client.issues.resolveInNextRelease(
          1,
          '323e4567-e89b-12d3-a456-426614174000',
        ),
      );
      // mock PATCH maps resolvedInNextRelease to the canonical 'resolved' status
      expect(issue.status).toBe('resolved');
    });
  });
});
