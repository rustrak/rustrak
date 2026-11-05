import type { CreateAuthToken } from '@rustrak/client';
import { mutationOptions } from '@tanstack/react-query';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';
import { tokenQueries } from './queries';

export const tokenMutations = {
  /** The answer carries the full token value, the only time it exists. */
  create: () =>
    mutationOptions({
      mutationFn: (body: CreateAuthToken) =>
        orThrow(api().tokens.create({ body })),
      meta: { invalidates: [tokenQueries.list().queryKey] },
    }),
  /** The token is the variable: the list deletes any row with one. */
  delete: () =>
    mutationOptions({
      mutationFn: (id: number) =>
        orThrow(api().tokens.delete({ path: { id } })),
      meta: { invalidates: [tokenQueries.all()] },
    }),
};
