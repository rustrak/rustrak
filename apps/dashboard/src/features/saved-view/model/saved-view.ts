/**
 * A saved view is a page's address under a name (#531). The server never
 * reads it: these turn a route's search into that text and back, so any page
 * whose state is its URL can save views.
 */
import type { SavedView } from '@rustrak/client';

/** Which page of results was open is where you were, not what you asked. */
const NOT_SAVED = new Set(['page']);

/** A route's search as the text a view stores: set keys only, in order. */
export function viewQuery(search: Record<string, unknown>): string {
  const params = new URLSearchParams();
  for (const key of Object.keys(search).sort()) {
    const value = search[key];
    if (value === undefined || value === null || value === '') continue;
    if (NOT_SAVED.has(key)) continue;
    params.set(key, String(value));
  }
  return params.toString();
}

/** A view's text as a search, for the route's own `validateSearch` to read. */
export function searchOf(query: string): Record<string, string> {
  return Object.fromEntries(new URLSearchParams(query));
}

/** An address with nothing set at all: no filter, no page, no open line. */
export function bareSearch(search: Record<string, unknown>): boolean {
  return Object.values(search).every(
    (value) => value === undefined || value === null || value === '',
  );
}

/**
 * The view whose address is the one on screen: `opened` when it still
 * matches, since two views can hold the same filters, else the first that
 * does. A query saved through the API is read the way the address writes it.
 */
export function activeView(
  views: SavedView[],
  search: Record<string, unknown>,
  opened?: number,
): SavedView | undefined {
  const query = viewQuery(search);
  const matches = (view: SavedView) =>
    viewQuery(searchOf(view.query)) === query;
  return (
    views.find((view) => view.id === opened && matches(view)) ??
    views.find(matches)
  );
}

/** The view the page opens on: the reader's own pick, then the project's. */
export function defaultView(views: SavedView[]): SavedView | undefined {
  return (
    views.find((view) => view.is_my_default) ??
    views.find((view) => view.is_project_default)
  );
}

export type ViewMenuAction =
  | 'save'
  | 'saveAs'
  | 'reset'
  | 'myDefault'
  | 'projectDefault'
  | 'delete';

export interface ViewMenu {
  /** The view on screen, or the one being edited. */
  current?: SavedView;
  /** `current`'s filters were changed since it was opened. */
  edited: boolean;
  actions: { kind: ViewMenuAction; view?: SavedView }[];
}

/**
 * What the views menu offers for the page as it is. `editing` is the view
 * last opened, kept while its filters change; `isAdmin` may update the
 * project, which is what changing someone else's shared view needs.
 */
export function viewMenu({
  views,
  search,
  editing,
  me,
  isAdmin,
}: {
  views: SavedView[];
  search: Record<string, unknown>;
  editing: number | undefined;
  me: number;
  isAdmin: boolean;
}): ViewMenu {
  const canEdit = (view: SavedView) => view.created_by === me || isAdmin;
  const active = activeView(views, search, editing);
  if (active) {
    const actions: ViewMenu['actions'] = [{ kind: 'myDefault', view: active }];
    if (isAdmin && active.visibility === 'project') {
      actions.push({ kind: 'projectDefault', view: active });
    }
    if (canEdit(active)) actions.push({ kind: 'delete', view: active });
    return { current: active, edited: false, actions };
  }

  const edited = views.find((view) => view.id === editing);
  const actions: ViewMenu['actions'] = [];
  if (edited && canEdit(edited)) actions.push({ kind: 'save', view: edited });
  if (viewQuery(search) !== '') actions.push({ kind: 'saveAs' });
  if (edited) actions.push({ kind: 'reset', view: edited });
  return { current: edited, edited: Boolean(edited), actions };
}
