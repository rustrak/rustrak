import { z } from 'zod';
import type { RustrakError } from '../errors.js';
import type { Result } from '../result.js';
import {
  offsetPaginatedResponseSchema,
  spanSchema,
  transactionDetailSchema,
  transactionSchema,
  transactionStatsSchema,
} from '../schemas/index.js';
import type {
  ListTransactionsOptions,
  OffsetPaginatedResponse,
  Span,
  Transaction,
  TransactionDetail,
  TransactionStats,
} from '../types/index.js';
import { BaseResource } from './base.js';

/**
 * Transactions API resource
 */
export class TransactionsResource extends BaseResource {
  /**
   * List transactions for a project with offset-based pagination (newest first)
   */
  async list(
    projectId: number,
    options?: ListTransactionsOptions,
  ): Promise<Result<OffsetPaginatedResponse<Transaction>, RustrakError>> {
    const searchParams: Record<string, string> = {};

    if (options?.page) {
      searchParams.page = String(options.page);
    }
    if (options?.per_page) {
      searchParams.per_page = String(options.per_page);
    }
    if (options?.name) {
      searchParams.name = options.name;
    }
    if (options?.op) {
      searchParams.op = options.op;
    }
    if (options?.status) {
      searchParams.status = options.status;
    }
    if (options?.environment) {
      searchParams.environment = options.environment;
    }
    if (options?.release) {
      searchParams.release = options.release;
    }

    return this.request(
      () =>
        this.http.get(`api/projects/${projectId}/transactions`, {
          searchParams,
        }),
      offsetPaginatedResponseSchema(transactionSchema),
    );
  }

  /**
   * Get the indexed spans extracted from a transaction, in waterfall order.
   */
  async getSpans(
    projectId: number,
    transactionId: string,
  ): Promise<Result<Span[], RustrakError>> {
    return this.request(
      () =>
        this.http.get(
          `api/projects/${projectId}/transactions/${transactionId}/spans`,
        ),
      z.array(spanSchema),
    );
  }

  /**
   * Get aggregate performance stats (count, p50/p95/p99 latency, failure rate)
   * grouped by transaction name + op, most frequent first, with offset-based
   * pagination — same shape as the other list endpoints.
   */
  async getStats(
    projectId: number,
    options?: { page?: number; per_page?: number; environment?: string },
  ): Promise<Result<OffsetPaginatedResponse<TransactionStats>, RustrakError>> {
    const searchParams: Record<string, string> = {};
    if (options?.page) {
      searchParams.page = String(options.page);
    }
    if (options?.per_page) {
      searchParams.per_page = String(options.per_page);
    }
    if (options?.environment) searchParams.environment = options.environment;

    return this.request(
      () =>
        this.http.get(`api/projects/${projectId}/transactions/stats`, {
          searchParams,
        }),
      offsetPaginatedResponseSchema(transactionStatsSchema),
    );
  }

  /**
   * Get aggregate stats for a single (transaction name, op) group — a direct
   * lookup that works regardless of how many groups the project has. Reports
   * `not_found` when the group has no transactions.
   */
  async getStatForGroup(
    projectId: number,
    name: string,
    op?: string,
    environment?: string,
  ): Promise<Result<TransactionStats, RustrakError>> {
    const searchParams: Record<string, string> = { name };
    if (op) {
      searchParams.op = op;
    }
    if (environment) searchParams.environment = environment;

    return this.request(
      () =>
        this.http.get(`api/projects/${projectId}/transactions/stats/group`, {
          searchParams,
        }),
      transactionStatsSchema,
    );
  }

  /**
   * Get a single transaction by ID with its full Sentry payload (spans,
   * contexts.trace, measurements, tags) for the performance detail view.
   */
  async get(
    projectId: number,
    transactionId: string,
  ): Promise<Result<TransactionDetail, RustrakError>> {
    return this.request(
      () =>
        this.http.get(
          `api/projects/${projectId}/transactions/${transactionId}`,
        ),
      transactionDetailSchema,
    );
  }
}
