# Dashboard end-to-end tests

Playwright drives the compiled dashboard in Chromium against the real Rust
server, on a fresh SQLite file, served from one origin the way Rustrak ships
(`start-server.sh`). Nothing is mocked: a test goes red when the page, the API
or the contract between them breaks.

```bash
pnpm turbo run build --filter=@rustrak/dashboard   # the suite serves dist/
pnpm --filter @rustrak/dashboard test:e2e           # starts the server itself
pnpm --filter @rustrak/dashboard exec playwright test --ui   # to debug one

# Coverage, as CI measures it for the README badge
pnpm --filter @rustrak/dashboard exec vite build --sourcemap
E2E_COVERAGE=1 pnpm --filter @rustrak/dashboard test:e2e   # -> coverage-e2e/lcov.info
```

Locally a server already on `:8790` is reused (`reuseExistingServer`), so
restart it after rebuilding the dashboard: it reads `index.html` at startup.

## What earns a test here

A test belongs here only if it is **a journey a user depends on, that no unit
or Rust test can see**: it needs the page, the router, the API and the
database at once. Rank by risk (impact × likelihood: data loss, security, a
user locked out, an error nobody hears about), push each rule to the lowest
layer that proves it, and stop. A page that merely renders is not a test:
it passes while every button on it is broken.

Each row below was proved by breaking that one behaviour in `src/` and
watching that test, and only that test, go red.

| Journey | Test | The bug it catches |
|---|---|---|
| **Getting started** | | |
| Create a project, send an error to the DSN the page shows | `onboarding` | A wrong DSN on the setup page |
| **Signing in** | | |
| A link to an issue survives the login | `auth` | Landing on `/projects` instead |
| A wrong password is refused, without saying which field | `auth` | Credentials error shown as a server failure |
| Signing out ends the session on the server | `auth` | A logout that only clears the browser |
| **Triage** | | |
| An issue shows the exception and the frame that threw | `issues` | A frame without its line |
| Resolve moves it from Open to Resolved, after a reload | `issues` | Resolve sending the wrong status |
| Resolving the selected issues leaves the rest open | `issues` | A bulk action that drops part of the selection |
| A deleted issue is gone, not hidden (the API answers 404) | `issues` | Delete aimed at the wrong issue |
| Assign to yourself shows "Me" in the list | `issues` | An assignment that sends nobody |
| A comment is kept after a reload | `issues` | A comment that loses its text |
| Every event of an issue can be read, not only the latest | `issues` | An unreachable previous event |
| A search narrows the list and survives a reload | `issues` | Search not read back from the URL |
| The all-projects list names each issue's project | `issues` | Rows labelled with another project |
| **Everything else an SDK sends** | | |
| A release lists the issues first seen in it | `telemetry` | The release page reading another release |
| A transaction shows its duration and opens its spans | `telemetry` | Samples filtered away |
| Logs can be searched by message | `telemetry` | The search ignored |
| Resolved feedback leaves the inbox | `telemetry` | Feedback muted instead of resolved |
| A failed check-in through the page's URL marks the monitor failing | `crons` | A failed run reported as OK |
| A new-issue alert POSTs to its webhook | `alerts` | An alert saved without its channel |
| **Settings and access** | | |
| A renamed project is listed by its new name | `settings` | The name never sent |
| A deleted project is gone and its DSN refuses events | `settings` | A delete that never reaches the server |
| A member sees a project only once added to it | `settings` | A member added without a role |
| An API token works until revoked | `settings` | A revoke that never reaches the server |
| A changed password replaces the old one | `settings` | The old password kept |
| The chosen language is kept on the account | `settings` | The language not saved |
| An invited member sets a password and is signed in | `team` | Accepting that does not sign in |
| On a phone, the tab bar reaches an issue | `mobile` | The tab pointing elsewhere |

Not here, on purpose: validation messages, formatting, a component's states,
every filter and every column. Those are Vitest's (`src/**/*.test.ts`) or the
server's (`cargo test`). Not covered yet: GitHub (needs an app), SSO (needs an
identity provider), the agents pages (need gen_ai spans) and session health.

## Rules

These follow the most used Playwright skill for coding agents
([testdino-hq/playwright-skill](https://github.com/testdino-hq/playwright-skill):
`qa/test-design.md`, `qa/test-review.md`) and Playwright's own
[best practices](https://playwright.dev/docs/best-practices).

1. **Assert the outcome, not that something is on screen.** Ask: *would this
   test pass if the feature were broken?* If the assertion could be written
   without knowing the requirement, it is not one. Read the result back from
   the server: reload, or ask the API.
2. **Setup through the API, the journey through the UI.** `support.ts` creates
   projects, users and data the way an SDK or a script would. Sign-in is done
   once in `auth.setup.ts`; only `auth.spec.ts` uses the login form.
3. **Unique data per test** (`unique()`), never shared rows: tests run in
   parallel against one database and nothing is cleaned up.
4. **Never `browser.newContext()` bare**: it inherits the admin session, and
   signing in from it ends that session for every other test. Use
   `freshContext()`.
5. **Locators a user would use**: `getByRole` with a name, `getByLabel`. No CSS
   classes, no XPath; `nth()` only where order is the behaviour. A
   `data-testid` only when there is no accessible name, and that is an
   accessibility bug first.
6. **Web-first assertions, no sleeps.** Before navigating away after an action,
   wait for its visible result: a `goto` or `reload` aborts a request still in
   flight. Background work (event digestion) is waited for with `expect.poll`.
   After choosing from a menu or a list, wait for it to close before clicking
   the dialog behind it. Biome's Playwright rules lint the rest.
7. **No `test.skip`, `test.fixme` or `.only` committed.** CI forbids `.only`.
8. **Prove it can fail.** Before merging a new test, break the behaviour it
   guards (one line in `src/`), rebuild, and watch that test go red. Then run
   it with `--repeat-each=5` to prove it does not flake.
