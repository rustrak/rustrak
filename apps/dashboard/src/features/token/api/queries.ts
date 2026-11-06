import { queryOptions } from '@tanstack/react-query';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';

export const tokenQueries = {
  all: () => ['tokens'] as const,
  /** Every token, masked. */
  list: () =>
    queryOptions({
      queryKey: [...tokenQueries.all(), 'list'],
      queryFn: ({ signal }) => orThrow(api().tokens.list({ signal })),
    }),
};
