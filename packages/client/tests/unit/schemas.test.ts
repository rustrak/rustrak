import { describe, expect, it } from 'vitest';
import {
  authTokenCreatedSchema,
  authTokenSchema,
  createProjectSchema,
  eventDetailSchema,
  issueSchema,
  paginatedResponseSchema,
  projectSchema,
  transactionSchema,
  updateProjectSchema,
  userReportSchema,
} from '../../src/schemas/index.js';

describe('Schema Validation', () => {
  describe('projectSchema', () => {
    it('should validate valid project data', () => {
      const validProject = {
        id: 1,
        name: 'Test Project',
        slug: 'test-project',
        sentry_key: '123e4567-e89b-12d3-a456-426614174000',
        dsn: 'http://123e4567-e89b-12d3-a456-426614174000@localhost:8080/1',
        stored_event_count: 100,
        digested_event_count: 95,
        rate_limited_event_count: 0,
        rate_limit_per_minute: null,
        rate_limit_per_hour: null,
        created_at: '2026-01-20T10:00:00.000Z',
        updated_at: '2026-01-20T10:00:00.000Z',
        platform: null,
      };

      const result = projectSchema.safeParse(validProject);
      expect(result.success).toBe(true);
    });

    it('should validate project with a detected platform string', () => {
      const validProject = {
        id: 1,
        name: 'Test Project',
        slug: 'test-project',
        sentry_key: '123e4567-e89b-12d3-a456-426614174000',
        dsn: 'http://123e4567-e89b-12d3-a456-426614174000@localhost:8080/1',
        stored_event_count: 100,
        digested_event_count: 95,
        rate_limited_event_count: 0,
        rate_limit_per_minute: null,
        rate_limit_per_hour: null,
        created_at: '2026-01-20T10:00:00.000Z',
        updated_at: '2026-01-20T10:00:00.000Z',
        platform: 'python',
      };

      const result = projectSchema.safeParse(validProject);
      expect(result.success).toBe(true);
      if (result.success) {
        expect(result.data.platform).toBe('python');
      }
    });

    it('should reject project with invalid UUID', () => {
      const invalidProject = {
        id: 1,
        name: 'Test Project',
        slug: 'test-project',
        sentry_key: 'not-a-uuid',
        dsn: 'http://localhost:8080/1',
        stored_event_count: 100,
        digested_event_count: 95,
        rate_limited_event_count: 0,
        rate_limit_per_minute: null,
        rate_limit_per_hour: null,
        created_at: '2026-01-20T10:00:00.000Z',
        updated_at: '2026-01-20T10:00:00.000Z',
      };

      const result = projectSchema.safeParse(invalidProject);
      expect(result.success).toBe(false);
    });

    it('should reject project with invalid datetime', () => {
      const invalidProject = {
        id: 1,
        name: 'Test Project',
        slug: 'test-project',
        sentry_key: '123e4567-e89b-12d3-a456-426614174000',
        dsn: 'http://localhost:8080/1',
        stored_event_count: 100,
        digested_event_count: 95,
        rate_limited_event_count: 0,
        rate_limit_per_minute: null,
        rate_limit_per_hour: null,
        created_at: 'not-a-date',
        updated_at: '2026-01-20T10:00:00.000Z',
      };

      const result = projectSchema.safeParse(invalidProject);
      expect(result.success).toBe(false);
    });

    it('should reject project with missing required fields', () => {
      const invalidProject = {
        id: 1,
        name: 'Test Project',
      };

      const result = projectSchema.safeParse(invalidProject);
      expect(result.success).toBe(false);
    });
  });

  describe('project quota fields', () => {
    const project = {
      id: 1,
      name: 'Test Project',
      slug: 'test-project',
      sentry_key: '123e4567-e89b-12d3-a456-426614174000',
      dsn: 'http://123e4567-e89b-12d3-a456-426614174000@localhost:8080/1',
      stored_event_count: 100,
      digested_event_count: 95,
      rate_limited_event_count: 7,
      rate_limit_per_minute: 300,
      rate_limit_per_hour: null,
      created_at: '2026-01-20T10:00:00.000Z',
      updated_at: '2026-01-20T10:00:00.000Z',
      platform: null,
    };

    it('keeps what the quota dropped and the project own limits', () => {
      const parsed = projectSchema.parse(project);
      expect(parsed.rate_limited_event_count).toBe(7);
      expect(parsed.rate_limit_per_minute).toBe(300);
      expect(parsed.rate_limit_per_hour).toBeNull();
    });

    it('sets a limit with a number and removes it with null', () => {
      expect(updateProjectSchema.parse({ rate_limit_per_minute: 300 })).toEqual(
        { rate_limit_per_minute: 300 },
      );
      expect(updateProjectSchema.parse({ rate_limit_per_hour: null })).toEqual({
        rate_limit_per_hour: null,
      });
      expect(
        updateProjectSchema.safeParse({ rate_limit_per_minute: 0 }).success,
      ).toBe(false);
    });
  });

  describe('createProjectSchema', () => {
    it('should validate create project with all fields', () => {
      const input = {
        name: 'New Project',
        slug: 'new-project',
      };

      const result = createProjectSchema.safeParse(input);
      expect(result.success).toBe(true);
    });

    it('should validate create project without optional slug', () => {
      const input = {
        name: 'New Project',
      };

      const result = createProjectSchema.safeParse(input);
      expect(result.success).toBe(true);
    });

    it('should reject empty name', () => {
      const input = {
        name: '',
      };

      const result = createProjectSchema.safeParse(input);
      expect(result.success).toBe(false);
    });
  });

  describe('issueSchema', () => {
    it('should validate valid issue data', () => {
      const validIssue = {
        id: '123e4567-e89b-12d3-a456-426614174000',
        project_id: 1,
        short_id: 'TEST-1',
        title: 'TypeError: Cannot read property',
        value: "Cannot read property 'x' of undefined",
        culprit: 'handleRequest',
        logger: '',
        first_seen: '2026-01-20T10:00:00.000Z',
        last_seen: '2026-01-20T11:00:00.000Z',
        event_count: 5,
        level: 'error',
        platform: 'javascript',
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
      };

      const result = issueSchema.safeParse(validIssue);
      expect(result.success).toBe(true);
    });

    it('should allow null level and platform', () => {
      const issue = {
        id: '123e4567-e89b-12d3-a456-426614174000',
        project_id: 1,
        short_id: 'TEST-1',
        title: 'Error',
        value: 'Something went wrong',
        culprit: '',
        logger: '',
        first_seen: '2026-01-20T10:00:00.000Z',
        last_seen: '2026-01-20T11:00:00.000Z',
        event_count: 5,
        level: null,
        platform: null,
        status: 'unresolved',
        substatus: null,
        priority: null,
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
      };

      const result = issueSchema.safeParse(issue);
      expect(result.success).toBe(true);
    });

    it('should default user_report_count to 0 when omitted', () => {
      const issue = {
        id: '123e4567-e89b-12d3-a456-426614174000',
        project_id: 1,
        short_id: 'TEST-1',
        title: 'Error',
        value: 'Something went wrong',
        culprit: '',
        logger: '',
        first_seen: '2026-01-20T10:00:00.000Z',
        last_seen: '2026-01-20T11:00:00.000Z',
        event_count: 5,
        level: null,
        platform: null,
        status: 'unresolved',
        substatus: null,
        priority: null,
        assigned_to: null,
        assignee_type: null,
        issue_type: 'error',
        issue_category: 'error',
        first_release: '',
        last_release: '',
        status_details: {},
        is_resolved: false,
        is_muted: false,
      };

      const result = issueSchema.parse(issue);
      expect(result.user_report_count).toBe(0);
    });
  });

  describe('userReportSchema', () => {
    it('should validate a report with a real email', () => {
      const report = {
        id: '123e4567-e89b-12d3-a456-426614174000',
        project_id: 1,
        issue_id: '223e4567-e89b-12d3-a456-426614174000',
        event_id: '323e4567-e89b-12d3-a456-426614174000',
        name: 'Jane Doe',
        email: 'jane@example.com',
        comments: 'It crashed when I clicked submit.',
        created_at: '2026-01-20T10:00:00.000Z',
      };

      const result = userReportSchema.safeParse(report);
      expect(result.success).toBe(true);
    });

    it('should accept an empty email — server stores "" for anonymous reports (DEFAULT \'\' / unwrap_or_default), matching real Sentry\'s own save_userreport(report.get("email", ""))', () => {
      const anonymousReport = {
        id: '123e4567-e89b-12d3-a456-426614174000',
        project_id: 1,
        issue_id: null,
        event_id: null,
        name: '',
        email: '',
        comments: 'Anonymous feedback.',
        created_at: '2026-01-20T10:00:00.000Z',
      };

      const result = userReportSchema.safeParse(anonymousReport);
      expect(result.success).toBe(true);
    });

    it('should reject a malformed non-empty email', () => {
      const report = {
        id: '123e4567-e89b-12d3-a456-426614174000',
        project_id: 1,
        issue_id: null,
        event_id: null,
        name: '',
        email: 'not-an-email',
        comments: 'x',
        created_at: '2026-01-20T10:00:00.000Z',
      };

      const result = userReportSchema.safeParse(report);
      expect(result.success).toBe(false);
    });
  });

  describe('eventDetailSchema', () => {
    it('should validate event with complex data object', () => {
      const validEvent = {
        id: '123e4567-e89b-12d3-a456-426614174000',
        event_id: '223e4567-e89b-12d3-a456-426614174000',
        issue_id: '323e4567-e89b-12d3-a456-426614174000',
        title: 'Error: Something went wrong',
        timestamp: '2026-01-20T10:00:00.000Z',
        ingested_at: '2026-01-20T10:00:01.000Z',
        level: 'error',
        platform: 'javascript',
        release: '1.0.0',
        environment: 'production',
        server_name: 'web-1',
        sdk_name: '@sentry/browser',
        sdk_version: '7.0.0',
        event_type: 'error',
        data: {
          exception: {
            values: [
              {
                type: 'Error',
                value: 'Something went wrong',
                stacktrace: {
                  frames: [],
                },
              },
            ],
          },
          request: {
            url: 'https://example.com/api',
            method: 'GET',
          },
        },
      };

      const result = eventDetailSchema.safeParse(validEvent);
      expect(result.success).toBe(true);
    });

    it('should handle empty data object', () => {
      const event = {
        id: '123e4567-e89b-12d3-a456-426614174000',
        event_id: '223e4567-e89b-12d3-a456-426614174000',
        issue_id: '323e4567-e89b-12d3-a456-426614174000',
        title: 'Error',
        timestamp: '2026-01-20T10:00:00.000Z',
        ingested_at: '2026-01-20T10:00:01.000Z',
        level: 'error',
        platform: 'javascript',
        release: '1.0.0',
        environment: 'production',
        server_name: 'web-1',
        sdk_name: '@sentry/browser',
        sdk_version: '7.0.0',
        event_type: 'error',
        data: {},
      };

      const result = eventDetailSchema.safeParse(event);
      expect(result.success).toBe(true);
    });
  });

  describe('transactionSchema', () => {
    const base = {
      id: '550e8400-e29b-41d4-a716-446655440000',
      event_id: '550e8400-e29b-41d4-a716-446655440000',
      transaction_name: '/api/checkout',
      timestamp: '2026-06-18T12:00:00.000Z',
      start_timestamp: '2026-06-18T11:59:59.000Z',
      duration_ms: 1000,
      platform: 'node',
      environment: 'production',
      release: '1.0.0',
      ingested_at: '2026-06-18T12:00:01.000Z',
    };

    it('accepts a Sentry event_id whose version nibble is not RFC 1-8', () => {
      // Sentry event ids are 32 random hex chars; once formatted as a UUID the
      // version nibble (here `9`) and variant are arbitrary. The list must not
      // be rejected wholesale because of this. Regression for empty Performance.
      const result = transactionSchema.safeParse({
        ...base,
        event_id: '5bda3e1e-66be-9c41-bf8a-1c97848e1092',
      });
      expect(result.success).toBe(true);
    });

    it('still rejects a malformed event_id', () => {
      const result = transactionSchema.safeParse({
        ...base,
        event_id: 'not-a-uuid',
      });
      expect(result.success).toBe(false);
    });

    it('accepts a null start_timestamp and duration', () => {
      const result = transactionSchema.safeParse({
        ...base,
        start_timestamp: null,
        duration_ms: null,
      });
      expect(result.success).toBe(true);
    });
  });

  describe('paginatedResponseSchema', () => {
    it('should validate paginated response with items', () => {
      const response = {
        items: [
          {
            id: 1,
            name: 'Project 1',
            slug: 'project-1',
            sentry_key: '123e4567-e89b-12d3-a456-426614174000',
            dsn: 'http://localhost:8080/1',
            stored_event_count: 100,
            digested_event_count: 95,
            rate_limited_event_count: 0,
            rate_limit_per_minute: null,
            rate_limit_per_hour: null,
            created_at: '2026-01-20T10:00:00.000Z',
            updated_at: '2026-01-20T10:00:00.000Z',
            platform: null,
          },
        ],
        next_cursor: 'eyJzb3J0IjoiZGlnZXN0X29yZGVyIn0=',
        has_more: true,
      };

      const result = paginatedResponseSchema(projectSchema).safeParse(response);
      expect(result.success).toBe(true);
    });

    it('should validate empty paginated response', () => {
      const response = {
        items: [],
        has_more: false,
      };

      const result = paginatedResponseSchema(projectSchema).safeParse(response);
      expect(result.success).toBe(true);
    });

    it('should allow undefined next_cursor', () => {
      const response = {
        items: [],
        next_cursor: undefined,
        has_more: false,
      };

      const result = paginatedResponseSchema(projectSchema).safeParse(response);
      expect(result.success).toBe(true);
    });
  });

  describe('authTokenSchema', () => {
    it('should validate auth token with all fields', () => {
      const token = {
        id: 1,
        token_prefix: 'abc12345...',
        description: 'My Token',
        created_at: '2026-01-20T10:00:00.000Z',
        last_used_at: '2026-01-20T11:00:00.000Z',
      };

      const result = authTokenSchema.safeParse(token);
      expect(result.success).toBe(true);
    });

    it('should allow null description and last_used_at', () => {
      const token = {
        id: 1,
        token_prefix: 'abc12345...',
        description: null,
        created_at: '2026-01-20T10:00:00.000Z',
        last_used_at: null,
      };

      const result = authTokenSchema.safeParse(token);
      expect(result.success).toBe(true);
    });
  });

  describe('authTokenCreatedSchema', () => {
    it('should validate newly created token with full token', () => {
      const token = {
        id: 1,
        token: 'abc123456789def',
        description: 'CI Token',
        created_at: '2026-01-20T10:00:00.000Z',
      };

      const result = authTokenCreatedSchema.safeParse(token);
      expect(result.success).toBe(true);
    });
  });
});
