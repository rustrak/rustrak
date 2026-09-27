/**
 * Reads for the token feature.
 */

import type { AuthToken, Result, RustrakError } from '@rustrak/client';
import { queryOptions } from '@tanstack/react-query';
import { scope } from '@/shared/api/query-client';
import { createClient } from '@/shared/api/rustrak';

/**
 * List all auth tokens (masked).
 * The full token is never returned after creation.
 *
 * @returns List of auth tokens with masked token values
 */

/**
 * List all auth tokens (masked).
 * The full token is never returned after creation.
 *
 * @returns List of auth tokens with masked token values
 */
export async function listTokens(): Promise<Result<AuthToken[], RustrakError>> {
  const client = await createClient();
  return client.tokens.list();
}

export const tokenQueries = {
  list: () => queryOptions({ queryKey: scope.tokens, queryFn: listTokens }),
};
