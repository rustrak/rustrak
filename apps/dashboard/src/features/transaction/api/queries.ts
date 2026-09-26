/**
 * Reads for the transaction feature.
 */

import type {
  ListTransactionsOptions,
  OffsetPaginatedResponse,
  Result,
  RustrakError,
  Transaction,
  TransactionDetail,
  TransactionStats,
} from '@rustrak/client';
import { Ok } from '@rustrak/client';
import { queryOptions } from '@tanstack/react-query';
import { scope } from '@/shared/api/query-client';
import { createClient } from '@/shared/api/rustrak';

export async function listTransactions(
  projectId: number,
  options?: ListTransactionsOptions,
): Promise<Result<OffsetPaginatedResponse<Transaction>, RustrakError>> {
  const client = await createClient();
  return client.transactions.list(projectId, options);
}

export async function getTransaction(
  projectId: number,
  transactionId: string,
): Promise<Result<TransactionDetail, RustrakError>> {
  const client = await createClient();
  return client.transactions.get(projectId, transactionId);
}

export async function getTransactionStats(
  projectId: number,
  options?: { page?: number; per_page?: number },
): Promise<Result<OffsetPaginatedResponse<TransactionStats>, RustrakError>> {
  const client = await createClient();
  return client.transactions.getStats(projectId, options);
}

/**
 * Aggregate metrics for one transaction group, or `null` when the group has
 * never been sampled.
 *
 * `not_found` is how the server says "no rows under this name", which is a real
 * empty answer and becomes `Ok(null)`. Every other failure stays a failure: the
 * previous `.catch(() => null)` also swallowed `network` and `server_error`, so
 * an outage rendered as "this endpoint has no metrics".
 */
export async function getTransactionStatForGroup(
  projectId: number,
  name: string,
  op?: string,
): Promise<Result<TransactionStats | null, RustrakError>> {
  const client = await createClient();
  const result = await client.transactions.getStatForGroup(projectId, name, op);

  if (!result.success && result.error.kind === 'not_found') {
    return Ok(null);
  }

  return result;
}

export const transactionQueries = {
  list: (projectId: number, options?: ListTransactionsOptions) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'transactions', 'list', options],
      queryFn: () => listTransactions(projectId, options),
    }),
  detail: (projectId: number, transactionId: string) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'transactions', transactionId],
      queryFn: () => getTransaction(projectId, transactionId),
    }),
  stats: (projectId: number, options?: { page?: number; per_page?: number }) =>
    queryOptions({
      queryKey: [...scope.project(projectId), 'transactions', 'stats', options],
      queryFn: () => getTransactionStats(projectId, options),
    }),
  groupStat: (projectId: number, name: string, op?: string) =>
    queryOptions({
      queryKey: [
        ...scope.project(projectId),
        'transactions',
        'group-stat',
        name,
        op,
      ],
      queryFn: () => getTransactionStatForGroup(projectId, name, op),
    }),
};
