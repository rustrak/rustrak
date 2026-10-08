import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createTestEnv, ok } from '../setup.js';

describe('storage tools', () => {
  let mockClient: any;
  let testEnv: Awaited<ReturnType<typeof createTestEnv>>;
  let callTool: Awaited<ReturnType<typeof createTestEnv>>['callTool'];

  const mockSummary = {
    total_db_size_bytes: 1048576,
    events_count: 120,
    transactions_count: 80,
    spans_count: 640,
    source_maps: {
      chunk_bytes: 350,
      source_file_bytes: 300,
      total_bytes: 650,
      file_count: 2,
    },
  };

  const mockProjects = [
    {
      project_id: 1,
      project_name: 'Test Project',
      events_count: 100,
      transactions_count: 80,
      spans_count: 640,
      source_maps_count: 2,
      estimated_bytes: 524288,
    },
  ];

  const mockCounts = {
    events: 20,
    transactions: 10,
    spans: 80,
    issues_removed: 3,
  };

  const mockStatus = (state: string) => ({
    state,
    removed: { ...mockCounts, logs: 0 },
    started_at: '2026-10-07T10:00:00Z',
    finished_at: state === 'running' ? null : '2026-10-07T10:05:00Z',
    error: null,
  });

  beforeEach(async () => {
    mockClient = {
      storage: {
        getSummary: vi.fn(),
        getProjects: vi.fn(),
        previewCleanup: vi.fn(),
        executeCleanup: vi.fn(),
        getCleanupStatus: vi.fn(),
        previewGcSourceMaps: vi.fn(),
        gcSourceMaps: vi.fn(),
      },
    };
    testEnv = await createTestEnv(mockClient);
    callTool = testEnv.callTool;
  });

  afterEach(async () => {
    await testEnv.mcpClient.close();
  });

  describe('get_storage_summary', () => {
    it('returns the instance-wide storage summary', async () => {
      mockClient.storage.getSummary.mockResolvedValue(ok(mockSummary));

      const result = await callTool({
        name: 'get_storage_summary',
        arguments: {},
      });

      expect(result.isError).toBeFalsy();
      const parsed = JSON.parse(result.content[0].text);
      expect(parsed.events_count).toBe(120);
      expect(parsed.source_maps.total_bytes).toBe(650);
    });
  });

  describe('get_storage_by_project', () => {
    it('returns the per-project breakdown', async () => {
      mockClient.storage.getProjects.mockResolvedValue(ok(mockProjects));

      const result = await callTool({
        name: 'get_storage_by_project',
        arguments: {},
      });

      expect(result.isError).toBeFalsy();
      const parsed = JSON.parse(result.content[0].text);
      expect(parsed).toHaveLength(1);
      expect(parsed[0].project_name).toBe('Test Project');
    });
  });

  describe('preview_storage_cleanup', () => {
    it('forwards older_than_days and project scope, returns dry-run counts', async () => {
      mockClient.storage.previewCleanup.mockResolvedValue(ok(mockCounts));

      const result = await callTool({
        name: 'preview_storage_cleanup',
        arguments: { older_than_days: 30, project_id: 1 },
      });

      expect(result.isError).toBeFalsy();
      const parsed = JSON.parse(result.content[0].text);
      expect(parsed.events).toBe(20);
      expect(mockClient.storage.previewCleanup).toHaveBeenCalledWith({
        older_than_days: 30,
        project_id: 1,
      });
    });

    it('forwards the data-type selection flags', async () => {
      mockClient.storage.previewCleanup.mockResolvedValue(ok(mockCounts));

      await callTool({
        name: 'preview_storage_cleanup',
        arguments: {
          older_than_days: 30,
          include_events: false,
          include_transactions: false,
          include_logs: true,
        },
      });

      expect(mockClient.storage.previewCleanup).toHaveBeenCalledWith(
        expect.objectContaining({
          older_than_days: 30,
          include_events: false,
          include_transactions: false,
          include_logs: true,
        }),
      );
    });
  });

  describe('execute_storage_cleanup', () => {
    it('refuses to run without confirm:true and does not call the client', async () => {
      const result = await callTool({
        name: 'execute_storage_cleanup',
        arguments: { older_than_days: 30 },
      });

      expect(result.isError).toBe(true);
      expect(mockClient.storage.executeCleanup).not.toHaveBeenCalled();
    });

    it('starts the destructive cleanup when confirm is true', async () => {
      mockClient.storage.executeCleanup.mockResolvedValue(
        ok(mockStatus('running')),
      );

      const result = await callTool({
        name: 'execute_storage_cleanup',
        arguments: { older_than_days: 30, confirm: true },
      });

      expect(result.isError).toBeFalsy();
      const parsed = JSON.parse(result.content[0].text);
      expect(parsed.state).toBe('running');
      expect(mockClient.storage.executeCleanup).toHaveBeenCalledWith({
        older_than_days: 30,
        project_id: undefined,
      });
    });

    it('forwards the data-type selection flags when confirmed', async () => {
      mockClient.storage.executeCleanup.mockResolvedValue(
        ok(mockStatus('running')),
      );

      await callTool({
        name: 'execute_storage_cleanup',
        arguments: {
          older_than_days: 30,
          confirm: true,
          include_events: false,
          include_transactions: true,
          include_logs: false,
        },
      });

      expect(mockClient.storage.executeCleanup).toHaveBeenCalledWith(
        expect.objectContaining({
          older_than_days: 30,
          include_events: false,
          include_transactions: true,
          include_logs: false,
        }),
      );
    });
  });

  describe('get_storage_cleanup_status', () => {
    it('returns the progress or outcome of the cleanup', async () => {
      mockClient.storage.getCleanupStatus.mockResolvedValue(
        ok(mockStatus('completed')),
      );

      const result = await callTool({
        name: 'get_storage_cleanup_status',
        arguments: {},
      });

      expect(result.isError).toBeFalsy();
      const parsed = JSON.parse(result.content[0].text);
      expect(parsed.state).toBe('completed');
      expect(parsed.removed.issues_removed).toBe(3);
    });
  });

  describe('preview_storage_source_maps_gc', () => {
    it('returns the orphaned files and bytes a GC would reclaim without deleting', async () => {
      mockClient.storage.previewGcSourceMaps.mockResolvedValue(
        ok({
          files_removed: 4,
          bytes_freed: 81920,
        }),
      );

      const result = await callTool({
        name: 'preview_storage_source_maps_gc',
        arguments: {},
      });

      expect(result.isError).toBeFalsy();
      const parsed = JSON.parse(result.content[0].text);
      expect(parsed.files_removed).toBe(4);
      expect(mockClient.storage.previewGcSourceMaps).toHaveBeenCalled();
      expect(mockClient.storage.gcSourceMaps).not.toHaveBeenCalled();
    });
  });

  describe('gc_storage_source_maps', () => {
    it('refuses to run without confirm:true and does not call the client', async () => {
      const result = await callTool({
        name: 'gc_storage_source_maps',
        arguments: {},
      });

      expect(result.isError).toBe(true);
      expect(mockClient.storage.gcSourceMaps).not.toHaveBeenCalled();
    });

    it('returns the orphaned files removed and bytes freed when confirm is true', async () => {
      mockClient.storage.gcSourceMaps.mockResolvedValue(
        ok({
          files_removed: 4,
          bytes_freed: 81920,
        }),
      );

      const result = await callTool({
        name: 'gc_storage_source_maps',
        arguments: { confirm: true },
      });

      expect(result.isError).toBeFalsy();
      const parsed = JSON.parse(result.content[0].text);
      expect(parsed.files_removed).toBe(4);
      expect(mockClient.storage.gcSourceMaps).toHaveBeenCalled();
    });
  });
});
