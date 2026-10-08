import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createTestEnv, fail, ok } from '../setup.js';

describe('issue tools', () => {
  let mockClient: any;
  let testEnv: Awaited<ReturnType<typeof createTestEnv>>;
  let callTool: Awaited<ReturnType<typeof createTestEnv>>['callTool'];

  beforeEach(async () => {
    mockClient = {
      issues: {
        list: vi.fn(),
        get: vi.fn(),
        updateState: vi.fn(),
        delete: vi.fn(),
        bulkUpdate: vi.fn(),
        bulkDelete: vi.fn(),
        getHashes: vi.fn(),
        getTagValues: vi.fn(),
        getAggregates: vi.fn(),
        getStats: vi.fn(),
        getActivity: vi.fn(),
        addComment: vi.fn(),
        setBookmark: vi.fn(),
        setSubscription: vi.fn(),
        markSeen: vi.fn(),
        listUserReports: vi.fn(),
        createUserReport: vi.fn(),
      },
      projects: { list: vi.fn(), get: vi.fn(), create: vi.fn() },
      events: { list: vi.fn(), get: vi.fn() },
      tokens: { list: vi.fn(), get: vi.fn(), create: vi.fn(), delete: vi.fn() },
      alertIntegrations: { list: vi.fn(), test: vi.fn() },
      alertRules: { list: vi.fn() },
    };
    testEnv = await createTestEnv(mockClient);
    callTool = testEnv.callTool;
  });

  afterEach(async () => {
    await testEnv.mcpClient.close();
  });

  describe('list_issues', () => {
    it('returns paginated issues for a project', async () => {
      const mockData = {
        items: [
          {
            id: 'abc123',
            title: 'TypeError: Cannot read property',
            is_resolved: false,
            is_muted: false,
            event_count: 5,
            user_count: 2,
            first_seen: '2024-01-01T00:00:00Z',
            last_seen: '2024-01-02T00:00:00Z',
            project_id: 1,
          },
        ],
        total: 1,
        page: 1,
        per_page: 25,
      };
      mockClient.issues.list.mockResolvedValue(ok(mockData));

      const result = await callTool({
        name: 'list_issues',
        arguments: { project_id: 1 },
      });

      expect(result.isError).toBeFalsy();
      expect(result.content[0]?.type).toBe('text');
      const parsed = JSON.parse(result.content[0].text);
      expect(parsed.items).toHaveLength(1);
      expect(mockClient.issues.list).toHaveBeenCalledWith(1, {
        page: undefined,
        per_page: undefined,
        filter: undefined,
      });
    });
  });

  describe('resolve_issue', () => {
    it('resolves an issue successfully', async () => {
      const mockIssue = {
        id: 'abc123',
        title: 'TypeError',
        is_resolved: true,
        is_muted: false,
        event_count: 1,
        user_count: 1,
        first_seen: '2024-01-01T00:00:00Z',
        last_seen: '2024-01-01T00:00:00Z',
        project_id: 1,
      };
      mockClient.issues.updateState.mockResolvedValue(ok(mockIssue));

      const result = await callTool({
        name: 'resolve_issue',
        arguments: { project_id: 1, issue_id: 'abc123' },
      });

      expect(result.isError).toBeFalsy();
      const parsed = JSON.parse(result.content[0].text);
      expect(parsed.is_resolved).toBe(true);
      expect(mockClient.issues.updateState).toHaveBeenCalledWith(1, 'abc123', {
        is_resolved: true,
      });
    });
  });

  describe('error handling', () => {
    it('returns isError: true on 404', async () => {
      mockClient.issues.get.mockResolvedValue(
        fail({
          kind: 'not_found',
          status: 404,
          message: 'Resource not found: issue-xyz',
        }),
      );

      const result = await callTool({
        name: 'get_issue',
        arguments: { project_id: 1, issue_id: 'xyz' },
      });

      expect(result.isError).toBe(true);
      expect(result.content[0]?.text).toMatch(/not found/i);
    });

    it('returns isError: true on 429 rate limit', async () => {
      mockClient.issues.list.mockResolvedValue(
        fail({
          kind: 'rate_limited',
          status: 429,
          message: 'Rate limit exceeded',
          retryAfter: 60,
        }),
      );

      const result = await callTool({
        name: 'list_issues',
        arguments: { project_id: 1 },
      });

      expect(result.isError).toBe(true);
      expect(result.content[0]?.text).toMatch(/60/);
    });

    it('forwards the q search parameter', async () => {
      mockClient.issues.list.mockResolvedValue(
        ok({ items: [], total_count: 0 }),
      );
      await callTool({
        name: 'list_issues',
        arguments: { project_id: 1, q: 'TypeError' },
      });
      expect(mockClient.issues.list).toHaveBeenCalledWith(1, {
        page: undefined,
        per_page: undefined,
        filter: undefined,
        q: 'TypeError',
      });
    });
  });

  describe('issue status & assignment (#165)', () => {
    it('update_issue_status sets canonical status + priority', async () => {
      mockClient.issues.updateState.mockResolvedValue(
        ok({ status: 'resolved' }),
      );
      const result = await callTool({
        name: 'update_issue_status',
        arguments: {
          project_id: 1,
          issue_id: 'abc',
          status: 'resolved',
          priority: 'high',
        },
      });
      expect(result.isError).toBeFalsy();
      expect(mockClient.issues.updateState).toHaveBeenCalledWith(1, 'abc', {
        status: 'resolved',
        priority: 'high',
      });
    });

    it('update_issue_status supports resolvedInNextRelease', async () => {
      mockClient.issues.updateState.mockResolvedValue(
        ok({ status: 'resolved' }),
      );
      await callTool({
        name: 'update_issue_status',
        arguments: {
          project_id: 1,
          issue_id: 'abc',
          status: 'resolvedInNextRelease',
        },
      });
      expect(mockClient.issues.updateState).toHaveBeenCalledWith(1, 'abc', {
        status: 'resolvedInNextRelease',
        priority: undefined,
      });
    });

    it('assign_issue assigns to a user', async () => {
      mockClient.issues.updateState.mockResolvedValue(ok({ assigned_to: 7 }));
      await callTool({
        name: 'assign_issue',
        arguments: { project_id: 1, issue_id: 'abc', assigned_to: 7 },
      });
      expect(mockClient.issues.updateState).toHaveBeenCalledWith(1, 'abc', {
        assigned_to: 7,
        assignee_type: undefined,
      });
    });
  });

  describe('bulk operations (#165)', () => {
    it('bulk_update_issues forwards ids + status', async () => {
      mockClient.issues.bulkUpdate.mockResolvedValue(ok({ updated: 2 }));
      const result = await callTool({
        name: 'bulk_update_issues',
        arguments: { project_id: 1, ids: ['a', 'b'], status: 'resolved' },
      });
      expect(result.isError).toBeFalsy();
      expect(mockClient.issues.bulkUpdate).toHaveBeenCalledWith(1, {
        ids: ['a', 'b'],
        status: 'resolved',
        priority: undefined,
      });
    });

    it('bulk_delete_issues forwards ids', async () => {
      mockClient.issues.bulkDelete.mockResolvedValue(ok({ deleted: 2 }));
      await callTool({
        name: 'bulk_delete_issues',
        arguments: { project_id: 1, ids: ['a', 'b'] },
      });
      expect(mockClient.issues.bulkDelete).toHaveBeenCalledWith(1, {
        ids: ['a', 'b'],
      });
    });

    it.each(['open', 'resolved', 'muted', 'all'])(
      'bulk_delete_issues forwards the %s filter',
      async (filter) => {
        mockClient.issues.bulkDelete.mockResolvedValue(ok({ deleted: 3 }));
        const result = await callTool({
          name: 'bulk_delete_issues',
          arguments: { project_id: 1, filter },
        });
        expect(result.isError).toBeFalsy();
        expect(mockClient.issues.bulkDelete).toHaveBeenCalledWith(1, {
          filter,
        });
        expect(JSON.parse(result.content[0].text)).toEqual({ deleted: 3 });
      },
    );

    it.each([
      {},
      { ids: [], filter: 'all' },
      { filter: 'everything' },
      { filter: null },
    ])('bulk_delete_issues rejects invalid selection %j', async (selection) => {
      const result = await callTool({
        name: 'bulk_delete_issues',
        arguments: { project_id: 1, ...selection },
      });
      expect(result.isError).toBe(true);
      expect(mockClient.issues.bulkDelete).not.toHaveBeenCalled();
    });

    it('bulk_delete_issues returns API failures for a filter', async () => {
      mockClient.issues.bulkDelete.mockResolvedValue(
        fail({
          kind: 'forbidden',
          status: 403,
          message: 'Insufficient project role',
        }),
      );
      const result = await callTool({
        name: 'bulk_delete_issues',
        arguments: { project_id: 1, filter: 'all' },
      });
      expect(result.isError).toBe(true);
      expect(mockClient.issues.bulkDelete).toHaveBeenCalledWith(1, {
        filter: 'all',
      });
      expect(result.content[0].text).toContain('Insufficient project role');
    });
  });

  describe('read sub-resources (#165)', () => {
    it('get_issue_hashes', async () => {
      mockClient.issues.getHashes.mockResolvedValue(ok([{ id: 1 }]));
      const result = await callTool({
        name: 'get_issue_hashes',
        arguments: { project_id: 1, issue_id: 'abc' },
      });
      expect(result.isError).toBeFalsy();
      expect(mockClient.issues.getHashes).toHaveBeenCalledWith(1, 'abc');
    });

    it('get_issue_tag_values forwards key', async () => {
      // Bare list, one entry per value (Sentry-compatible shape).
      mockClient.issues.getTagValues.mockResolvedValue(ok([]));
      await callTool({
        name: 'get_issue_tag_values',
        arguments: { project_id: 1, issue_id: 'abc', key: 'browser' },
      });
      expect(mockClient.issues.getTagValues).toHaveBeenCalledWith(
        1,
        'abc',
        'browser',
      );
    });

    it('get_issue_aggregates', async () => {
      mockClient.issues.getAggregates.mockResolvedValue(
        ok({
          user_count: 2,
          tags: [],
        }),
      );
      await callTool({
        name: 'get_issue_aggregates',
        arguments: { project_id: 1, issue_id: 'abc' },
      });
      expect(mockClient.issues.getAggregates).toHaveBeenCalledWith(1, 'abc');
    });

    it('get_issue_stats forwards window', async () => {
      mockClient.issues.getStats.mockResolvedValue(ok({ data: [] }));
      await callTool({
        name: 'get_issue_stats',
        arguments: { project_id: 1, issue_id: 'abc', window: '30d' },
      });
      expect(mockClient.issues.getStats).toHaveBeenCalledWith(1, 'abc', '30d');
    });

    it('get_issue_activity', async () => {
      mockClient.issues.getActivity.mockResolvedValue(ok([]));
      await callTool({
        name: 'get_issue_activity',
        arguments: { project_id: 1, issue_id: 'abc' },
      });
      expect(mockClient.issues.getActivity).toHaveBeenCalledWith(1, 'abc');
    });
  });

  describe('social tools (#165)', () => {
    it('comment_on_issue', async () => {
      mockClient.issues.addComment.mockResolvedValue(ok({ type: 'note' }));
      await callTool({
        name: 'comment_on_issue',
        arguments: { project_id: 1, issue_id: 'abc', text: 'hi' },
      });
      expect(mockClient.issues.addComment).toHaveBeenCalledWith(1, 'abc', {
        text: 'hi',
      });
    });

    it('bookmark_issue', async () => {
      mockClient.issues.setBookmark.mockResolvedValue(
        ok({ is_bookmarked: true }),
      );
      await callTool({
        name: 'bookmark_issue',
        arguments: { project_id: 1, issue_id: 'abc', enabled: true },
      });
      expect(mockClient.issues.setBookmark).toHaveBeenCalledWith(
        1,
        'abc',
        true,
      );
    });

    it('subscribe_issue', async () => {
      mockClient.issues.setSubscription.mockResolvedValue(
        ok({
          is_subscribed: false,
        }),
      );
      await callTool({
        name: 'subscribe_issue',
        arguments: { project_id: 1, issue_id: 'abc', enabled: false },
      });
      expect(mockClient.issues.setSubscription).toHaveBeenCalledWith(
        1,
        'abc',
        false,
      );
    });

    it('mark_issue_seen', async () => {
      mockClient.issues.markSeen.mockResolvedValue(ok({ has_seen: true }));
      await callTool({
        name: 'mark_issue_seen',
        arguments: { project_id: 1, issue_id: 'abc' },
      });
      expect(mockClient.issues.markSeen).toHaveBeenCalledWith(1, 'abc');
    });

    it('list_user_reports', async () => {
      mockClient.issues.listUserReports.mockResolvedValue(ok([]));
      await callTool({
        name: 'list_user_reports',
        arguments: { project_id: 1, issue_id: 'abc' },
      });
      expect(mockClient.issues.listUserReports).toHaveBeenCalledWith(1, 'abc');
    });

    it('submit_user_report', async () => {
      mockClient.issues.createUserReport.mockResolvedValue(ok({ id: 'r1' }));
      await callTool({
        name: 'submit_user_report',
        arguments: {
          project_id: 1,
          issue_id: 'abc',
          name: 'Jane',
          comments: 'broke',
        },
      });
      expect(mockClient.issues.createUserReport).toHaveBeenCalledWith(
        1,
        'abc',
        {
          name: 'Jane',
          email: undefined,
          comments: 'broke',
          event_id: undefined,
        },
      );
    });
  });
});
