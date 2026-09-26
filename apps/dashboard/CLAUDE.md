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
mounts `dist/` at `/` and keeps `/api`, `/auth`, `/health`, `/docs` and
`/api-docs` for itself, so the same Actix process answers the page and the API.

In development Vite owns the origin and proxies those same five prefixes to the
server. The list lives in `vite.config.ts` and is the same one the server
refuses to answer with the application shell; **the two must not drift**, or a
route works in one environment and 404s in the other.
`src/__tests__/architecture/api-prefixes.test.ts` reads both files and fails
the build if they disagree.

Everything follows from that:

- `createClient()` builds against `window.location.origin`
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
| `anonymous` | the server answered 401 | redirects to `/login` |
| `unavailable` | network, timeout, 5xx, 403, bad schema | renders "the server did not answer" |

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

Every read goes through **TanStack Query**, and the route decides when it
starts. The pattern, in every route:

- `features/<slice>/api/queries.ts` keeps its fetchers and exports a
  `queryOptions` factory next to them (`issueQueries.list(...)`). Keys start
  from the roots in `shared/api/query-client.ts`: everything that belongs to
  one project sits under `scope.project(id)`.
- The loader calls `context.queryClient.ensureQueryData(...)` for what the
  screen cannot render without, all in parallel, and returns only what `head`
  needs. Panels that should appear on their own (the overview tiles, the
  storage page) are `prefetchQuery` without `await`, read with `useQuery` and
  a skeleton.
- The component reads the same options with `useSuspenseQuery`, never
  `Route.useLoaderData()` for data: that way a write elsewhere updates it.
- A write invalidates its scope (`invalidateProject`, `invalidateIssues`,
  `invalidate(scope.tokens)`), never `router.invalidate()`, which re-ran every
  loader on screen.

A loader is not code-split, so whatever it imports ships in the entry. Keep
query options in modules with no UI in them (`overview-queries.ts` exists for
exactly this reason).

Every method on `@rustrak/client` returns a `Result` and never throws for an
expected failure, so a query caches the `Result` as-is and the component
renders both branches (`combine()` in `shared/lib/results.ts` merges several).
Do not `unwrap`. A read that answers `unauthenticated` is noticed once, in
`router.tsx`, which clears the session and sends the reader to `/login` with
`?redirect=`. The cache is dropped whenever the signed-in reader changes.

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
- `translator()` is the same catalogue outside React, for a route's `head`.
  TanStack resolves titles before a component tree exists.
- A new user-facing sentence goes in **all five** `messages/*.json`.
  `locale-completeness` and `message-keys` fail the build otherwise.

## Rules the CI enforces

`src/__tests__/architecture/` holds rule files written on
[archunit](https://github.com/lukasniessen/archunitts). They run in `pnpm test`
and cover layer direction, slice isolation, barrel files, the portable core,
`routes/`'s shape, locale completeness, the client `Result` shape, the version
staying behind the gate, that nothing imports Next any more, and that the API
prefix list has not drifted between Vite and the server.

**Read the rule before working around it.** Each file documents what it protects
and why it exists.

## Conventions

- **Navigation is the router's.** `Link` and `useNavigate` come straight from
  `@tanstack/react-router`, addressed by `to`/`params`/`search`, so a wrong
  path, param or search value fails `tsc`. The `href` shims are gone.
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
