/**
 * Reads for the transaction feature, called straight from Server Components.
 *
 * `import 'server-only'` is a build-time poison pill rather than a directive:
 * if this module reaches the client bundle the build fails, instead of shipping
 * a browser bundle that holds the session cookie.
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
  options?: { page?: number; per_page?: number; environment?: string },
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
  environment?: string,
): Promise<Result<TransactionStats | null, RustrakError>> {
  const client = await createClient();
  const result = await client.transactions.getStatForGroup(
    projectId,
    name,
    op,
    environment,
  );

  if (!result.success && result.error.kind === 'not_found') {
    return Ok(null);
  }

  return result;
}
