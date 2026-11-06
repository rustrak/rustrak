/**
 * Which form a provider's page shows, if any: a new integration, a stored one,
 * or none, which is the list. Only a form its reader may submit.
 */
export function providerEditing<T extends { id: number }>(
  edit: 'new' | number | undefined,
  integrations: T[],
  can: { create: boolean; update: boolean },
): T | 'new' | null {
  if (edit === 'new' || (edit === undefined && integrations.length === 0)) {
    return can.create ? 'new' : null;
  }
  // An id that is gone -- deleted in another tab -- is the list.
  return can.update ? (integrations.find((i) => i.id === edit) ?? null) : null;
}
