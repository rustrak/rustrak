/**
 * Which optional columns to leave out so the table fits its card.
 *
 * Columns with a `drop` order go first to last, and only while the sum of
 * widths is still wider than `available`. Unmeasured (`undefined`), nothing
 * is dropped. What still does not fit scrolls sideways, as before.
 */
export function droppedToFit(
  columns: readonly { id: string; width: number; drop?: number }[],
  available: number | undefined,
): Set<string> {
  const dropped = new Set<string>();
  if (available === undefined) return dropped;
  let total = columns.reduce((sum, column) => sum + column.width, 0);
  const optional = columns
    .filter((column) => column.drop !== undefined)
    .sort((a, b) => (a.drop ?? 0) - (b.drop ?? 0));
  for (const column of optional) {
    if (total <= available) break;
    dropped.add(column.id);
    total -= column.width;
  }
  return dropped;
}
