import type { SidebarProjects } from '@rustrak/client';

interface Listed {
  id: number;
  name: string;
}

/**
 * The reader's manual order: the ids they placed, then every project they
 * never placed in the order they came, so a new project shows up on its own.
 */
export function inManualOrder<P extends { id: number }>(
  projects: P[],
  order: number[],
): P[] {
  const byId = new Map(projects.map((p) => [p.id, p]));
  const placed = order.flatMap((id) => byId.get(id) ?? []);
  const seen = new Set(order);
  return [...placed, ...projects.filter((p) => !seen.has(p.id))];
}

/**
 * The projects the workspace sidebar lists, as the reader set it up: the
 * hidden ones left out, the rest by name, by their own order, or (for
 * `activity`) in the order given, which is the list sorted by last issue.
 */
export function sidebarProjects<P extends Listed>(
  projects: P[],
  prefs: SidebarProjects,
): P[] {
  const hidden = new Set(prefs.hidden);
  const sorted =
    prefs.sort === 'manual'
      ? inManualOrder(projects, prefs.order)
      : prefs.sort === 'name'
        ? [...projects].sort((a, b) =>
            a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }),
          )
        : projects;
  return sorted.filter((p) => !hidden.has(p.id));
}

/** `order` with `id` moved to index `to`, clamped to the list. */
export function moveTo(order: number[], id: number, to: number): number[] {
  const rest = order.filter((x) => x !== id);
  rest.splice(Math.max(0, Math.min(to, rest.length)), 0, id);
  return rest;
}

/**
 * How many places row `index` slides while the row at `from` is dragged over
 * index `to`: the rows in between move one place toward where it came from,
 * opening the gap it will drop into.
 */
export function makeRoom(index: number, from: number, to: number): number {
  if (from < to && index > from && index <= to) return -1;
  if (to < from && index >= to && index < from) return 1;
  return 0;
}

/**
 * What the sidebar shows after a save failed: the setup the server last
 * confirmed, unless another save is queued behind it, which carries this
 * change too and settles it either way.
 */
export function afterFailedSave(
  shown: SidebarProjects | undefined,
  confirmed: SidebarProjects | undefined,
  laterSaveQueued: boolean,
): SidebarProjects | undefined {
  return laterSaveQueued || !confirmed ? shown : confirmed;
}
