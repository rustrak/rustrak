# Rustrak Dashboard

React 19, TanStack Router in file-based mode, Vite, Tailwind and shadcn/ui,
`use-intl` for `en`, `zh`, `fr`, `es` and `ro`. Root context: `/CLAUDE.md`.

It compiles to static files and **the Rust server hands them out**. There is no
Node process in production, no second container, and no separate origin. The
one alternative is `Dockerfile` here: the same `dist/` behind nginx, proxying
the API prefixes to `RUSTRAK_API_URL`, for a dashboard hosted away from its
server. The browser still sees one origin either way.

```bash
pnpm dev --filter=@rustrak/dashboard          # Vite on :3000, proxying to the server
pnpm build --filter=@rustrak/dashboard        # -> dist/
pnpm test --filter=@rustrak/dashboard         # architecture rules + the portable core
```

The code is organised by **domain, not by technical type**, following a reduced
[Feature-Sliced Design](https://feature-sliced.design). There is no
`components/`, `hooks/`, `actions/` or `lib/` at the root of `src/`. They were
removed deliberately. Read this file before adding one back.

## Three layers

A layer imports only from layers strictly below it, never upward.

```
src/
├── routes/       routing and composition. TanStack lives here and nowhere else.
│   └── <route>.tsx        and -components/ for anything the route alone needs
├── features/     the domain, one slice per business concept
│   └── <slice>/
│       ├── ui/components/  components whose props name a domain type
│       ├── ui/hooks/
│       ├── api/queries.ts   reads
│       ├── api/mutations.ts writes
│       ├── model/          types and domain logic. No React, no router.
│       └── lib/            derived logic. No React, no router.
└── shared/       no slices, segments directly
    ├── ui/components/  primitives, with shadcn output under components/shadcn/
    ├── ui/hooks/
    ├── lib/            pure helpers
    ├── api/            the client, the session store
    ├── i18n/           use-intl wiring, messages/ underneath
    └── config/         constants and static tables
```

The slices come from the resources `@rustrak/client` exposes and the routes that
consume them. The non-obvious groupings carry the reasoning: `project` absorbs
stats, because stats are aggregates *of a project*; `release` absorbs sessions,
because release health *is* sessions grouped by release; `user` absorbs team,
members and invitations, because all three are people and their access.

**The session is in `shared/api`, not in `features/user`,** and that is the rule
rather than an exception to it. Three unrelated things need to know who is
asking before anything renders: the router's guard, `shared/i18n` (the reader's
language and zone are columns on the user row), and the account screens. A type
two consumers both need goes *down*, never sideways.

## The one idea: the browser only ever talks to its own origin

In production that is literally true. `apps/server/src/routes/dashboard.rs`
mounts `dist/` at `/` and keeps the API prefixes (`/api`, `/auth`, `/mcp`,
`/.well-known`…) for itself, so the same Actix process answers the page and the
API.

In development Vite owns the origin and proxies those same prefixes to the
server. The list lives in `vite.config.ts` and is the same one the server
refuses to answer with the application shell; **the two must not drift**, or a
route works in one environment and 404s in the other.
`src/__tests__/architecture/api-prefixes.test.ts` reads both files and fails
the build if they disagree.

Everything follows from that:

- `api()` builds the client against `window.location.origin`
  (`src/shared/api/rustrak.ts`). No API URL to configure, in either environment.
- The session cookie is first-party in both, so no `SameSite=None`, no
  third-party cookie policy to fight, and nothing parses `Set-Cookie` by hand.
- No CORS on the dashboard's path at all. The server's permissive CORS exists
  for SDK ingestion, not for this.

There is no override. A bundle hosted away from its server is `Dockerfile`
here: nginx proxying the same prefixes, so all three properties hold there
too. Calling the API cross-origin would give all three up, which is why no
`VITE_RUSTRAK_API_URL` exists.

## Who gets in

Everything behind a session lives under `src/routes/_authenticated/`, a pathless
layout route whose `beforeLoad` is the only guard in the application. Adding a
page means putting the file in that folder. **The check never goes in a
component:** one that redirects has already painted what it was protecting.

`src/shared/api/session.ts` reads the server's answer as **three** states:

| | means | the guard |
|---|---|---|
| `authenticated` | `/auth/me` returned a user | renders the page |
| `anonymous` | the server answered 401 | redirects to `/login`, leaving its address for `/` |
| `unavailable` | network, timeout, 5xx, 403, bad schema | renders "the server did not answer" |

Every way in (the password form, SSO, SSO's account linking) lands on `/`
afterwards, so the guard leaves the address it refused in `sessionStorage`
(`shared/api/return-path.ts`) and `/` resumes it. A link to an issue from Slack
opens the issue once the reader signs in, not the projects list. The consent
page at `/oauth/authorize` guards itself and uses the same hand-off.

The third row matters: only `anonymous` means signed out. Collapsing a dropped
connection into it bounces to `/login`, where the login request fails for the
same reason, on a loop.

The store is a module singleton, so `shared/i18n` can read it during bootstrap
before a router exists. `ensure()` memoises the in-flight promise — nested
guards would otherwise each issue a `/auth/me` — and drops only `unavailable`,
which is the one answer a retry can change.

Screens below the guard read the user with `useSessionUser()` rather than
fetching it again.

## Where the data comes from

**TanStack Query owns every read and write; the router decides when they
start.** This is the router's documented composition with Query
(`router-query`), and it is not optional: a screen never holds server data in
`useState`, never calls the API from an effect, and never re-runs loaders to
see a write.

One `QueryClient` (`shared/api/query.ts`), created in `main.tsx` above the
router and passed to it as context. Its defaults are deliberate:

- `staleTime` 30s. Within it, going back shows what was there without asking;
  past it, it still shows that at once and refetches behind it.
- `retry: false`. `@rustrak/client` already retries what a retry can fix.
- `defaultPreloadStaleTime: 0` on the router, so hovering a link starts its
  reads and the cache, not the router, decides whether they are fresh.

### Reads: `queryOptions` factories in `features/<slice>/api/queries.ts`

```ts
export const issueQueries = {
  all: (projectId: number) => [...projectScope(projectId), 'issues'] as const,
  list: (projectId: number, query: IssuesListQuery) =>
    queryOptions({
      queryKey: [...issueQueries.all(projectId), 'list', query],
      queryFn: ({ signal }) =>
        orThrow(api().issues.list({ path: { project_id: projectId }, query, signal })),
    }),
};
```

- **Keys are hierarchical** and everything of a project starts with
  `projectScope(id)` (`shared/api/keys.ts`), so invalidating
  `projectScope(id)` refreshes all of a project and a slice's `all(...)`
  refreshes the slice. Instance-wide data starts with its own root
  (`['storage']`, `['team']`). Slices cannot import each other; the shared
  prefix is why `projectScope` lives in `shared`.
- **`signal` always reaches the client**, so a read nobody waits for any more
  (a superseded filter, a page left) is cancelled, not left running.
- **`orThrow`** is the one seam between the client's `Result` and Query: it
  throws the failure as an `ApiError` carrying the `RustrakError` whole.
  `ApiError` is the registered `defaultError`, so `query.error.error` is typed.

### Loaders start reads and never wait

**A click navigates at once, and the page shows its own skeleton while its
reads are in flight.** A loader that awaits makes the router keep the
*previous* page on screen for the whole wait, with nothing saying the click
registered. So a loader is an event handler: it starts every read the page
will make, as early as possible (a hovered link runs it too), and returns
nothing.

```ts
loader: ({ context: { queryClient }, params, deps }) => {
  const id = Number.parseInt(params.id, 10);
  void queryClient.query(issueQueries.list(id, deps)).catch(noop);
  void queryClient.query(issueQueries.stats(id, deps.period)).catch(noop);
},
pendingComponent: IssuesLoading,
component: IssuesPage,
```

- **No `async`, no `await`, no return value.** `queryClient.query` respects
  `staleTime`, so a fresh entry is not refetched. `.catch(noop)` because the
  failure belongs to the component that reads it, not to the loader.
  `prefetchQuery` and `ensureQueryData` are deprecated in Query; do not use
  them.
- **Every read the page makes is started here**, critical or not, so nothing
  waits for the component to render first. Only the open tab's reads: a tab
  the reader is not on is not fetched.
- `loaderDeps` holds exactly the search keys the loader reads.
- A redirect that depends on data (an issue opening on its latest event) is
  a component that suspends on the read and renders `<Navigate replace />`,
  not a `beforeLoad` that awaits it.

### Components decide what the page waits for

- **What the page cannot paint without**: `useSuspenseQuery(options)`. It
  picks up the read the loader started; the route's `pendingComponent` is
  shown until it lands, and a failure goes to the route's error boundary
  (`RouteError`, which renders it through `LoadFailure`). A missing record is
  `throw notFound()` from the component.
- **What can arrive later** (a chart, a rail, a panel): `useQuery(options)`
  rendered through `QueryState` (`shared/ui/components/query-state.tsx`):
  its placeholder while pending, its `LoadFailure` when it failed, the data
  otherwise. A panel that used to degrade (an issue with no tags renders like
  a failed tags read) reads `query.data ?? fallback` instead, on purpose and
  visibly.
- Screen-local reads with no route (a dialog's preview, the command bar's
  search): `useQuery` in the component, with `enabled` for "not yet".

### Every route that suspends has its own skeleton

`pendingComponent` is the route's own Suspense fallback, mounted in the frame
after the click. Without one the suspension climbs to the boundary above and
blanks a whole layout. `route-loading.test.ts` fails a route that suspends
without one.

- **The page's chrome is real, only the data is skeleton.** The header with
  its static title and tabs renders as itself; `Skeleton` from `@rustrak/ui`
  stands where data goes, at the size of what replaces it. A list renders its
  real table with no page: `DataTable` draws skeleton rows under the real
  header (`IssuesTable` takes `page?` for this).
- Share the layout between the page and its skeleton: one screen component
  that takes the data as optional props, rendered by the page with it and by
  the `pendingComponent` without (`routes/.../issues/index.tsx`).
- Wrap it in `PageLoading` (`shared/ui/components/page-loading.tsx`): it
  announces the wait once, and draws no box, so the skeleton lays out like the
  page it stands for.

### The same route at a new address shows the previous one, dimmed

A filter, a page, a tab, the next event: the route stays, so its skeleton must
not flash on every keystroke. The component reads the address through
`useDeferredValue`, the documented React pattern for showing stale content
while fresh content loads:

```ts
const live = Route.useSearch();
const search = useDeferredValue(live);
const { data: page } = useSuspenseQuery(issueQueries.list(id, search));
// `search !== live` while the next page loads: rows stay, dimmed.
```

Controls read `live`, so they answer the click at once; data reads the
deferred value. Never the router's `isLoading` or `status`: it is never
pending for this, because loaders do not wait.

### Titles

A page renders its own `<title>`, which React 19 hoists into the document and
keeps current as its data changes. There is no route `head` and no
`<HeadContent />`: `head` can only read loader data, and loaders return none.

### Writes: `mutationOptions` factories in `features/<slice>/api/mutations.ts`

```ts
export const issueMutations = {
  update: (projectId: number, issueId: string) =>
    mutationOptions({
      mutationFn: (body: UpdateIssueState) =>
        orThrow(api().issues.update({ path: { project_id: projectId, issue_id: issueId }, body })),
      meta: { invalidates: [issueQueries.all(projectId)] },
    }),
};
```

- **A mutation names what it changed in `meta.invalidates`**; the cache
  refetches those keys once it settles (`shared/api/query.ts`), and
  `mutateAsync` resolves after they are back. Nothing calls `router.invalidate()`
  to see a write, and `invalidateQueries` by hand is only for a change no
  mutation settles: a background job (a storage cleanup) finishing.
- **A delete drops what it deleted** in its factory's `onSuccess`
  (`client.removeQueries`), before the invalidation refetches: a still-mounted
  page would otherwise ask again for a record that can only answer 404.
- Components use `useMutation(factory(...))`: `isPending` for the button, the
  `ApiError` in `onError` (a toast) or in a form's submit handler (field
  errors through `form-errors.ts`).
- An optimistic update writes the cache in `onMutate` and restores it in
  `onError`; the invalidation reconciles it either way.

### What is not a query

The session (`shared/api/session.ts`) stays a store. The router's guard and
`shared/i18n` both need it before a router or a `QueryClient` exists, and
reading it through two caches would let them disagree.

## The URL is the state

`validateSearch` is where a query string becomes typed search, and **whatever it
returns is the canonical URL**. So it never invents a value: `searchPage` answers
`undefined` for an absent `?page=`, and the `?? 1` lives at the call site. A
validator that answered `1` would write `?page=1` into an address that never had
one, and a shared link would come back changed.

An unrecognised value is dropped rather than passed on, because the API ignores
what it cannot parse and answers unfiltered — leaving a filter in the URL that
no control shows as selected.

## i18n

`shared/i18n/intl.ts` resolves the locale, the timezone and the named formats
**once per reader** and republishes when they change either. It is what Next's
`getRequestConfig` was, minus the request: the language comes from
`users.language`, then the browser, then English; the zone from `users.timezone`,
then UTC.

- No date or number formatting library. Everything goes through `use-intl`'s
  `useFormatter`, and the option sets are named once in `shared/i18n/config.ts`.
- A new user-facing sentence goes in **all five** `messages/*.json`.
  `locale-completeness` and `message-keys` fail the build otherwise.

## Rules the CI enforces

`src/__tests__/architecture/` holds rule files written on
[archunit](https://github.com/lukasniessen/archunitts). They run in `pnpm test`
and cover layer direction, slice isolation, barrel files, the portable core,
`routes/`'s shape, locale completeness, the client `Result` shape, the version
staying behind the gate, that nothing imports Next any more, that the API
prefix list has not drifted between Vite and the server, and that routes load
behind their own skeleton without making the router wait.

**Read the rule before working around it.** Each file documents what it protects
and why it exists.

## Conventions

- **Navigation is the router's.** `shared/ui/components/link.tsx` and
  `shared/ui/hooks/use-router.ts` are `href`-addressed shims over it, kept so
  the migration touched an import line rather than forty call sites. Addressing
  routes by `to`/`params` instead is a real improvement and a separate change.
- shadcn output is CLI output. Compose from outside instead of editing it.
- Prefer solving it in the event handler before reaching for an effect.
- `routeTree.gen.ts` is generated by the Vite plugin. It is committed, excluded
  from Biome, and never edited by hand.

## Where the build ends up

`vite build` writes `dist/`. `scripts/bundle-dashboard.sh` copies it to
`apps/server/static`, which is what `RUSTRAK_DASHBOARD_DIR` defaults to, so
`cargo run` from `apps/server` picks it up with no configuration. Turbo runs
that copy as part of `@rustrak/server#build`.

A missing build is not an error anywhere in that chain: the server is a complete
product without a UI, and `cargo build` alone has to keep working for anyone who
never installs Node.
