# docs

rustrak.dev: the landing, the documentation, the blog, the changelog, the
roadmap and the benchmark pages. Astro, fully static, deployed to GitHub Pages
by `.github/workflows/deploy-docs.yml`.

```bash
pnpm --filter docs dev       # :3001
pnpm --filter docs build     # astro build, then Pagefind indexes dist/
pnpm --filter docs test      # node --test src/lib/*.test.ts
```

## Rules

- **Zero JavaScript by default.** Pages are `.astro`. React exists for two
  islands only: the interactive demo player (`client:visible`) and the Scalar
  API reference (`client:only`). `@rustrak/ui` components render at build
  time and ship nothing.
- **Motion is CSS.** `data-reveal` elements animate when an
  IntersectionObserver (`layouts/base.astro`) adds `.in`. Anything on screen
  from the first frame passes `onLoad` to `Reveal`/`Words`, so it plays with
  the page instead of waiting for the observer. Overlays (drawer, search,
  menus) animate with `@starting-style` in `styles/docs.css`. Honour
  `prefers-reduced-motion` in every new animation.
- **Content lives in `content/`.** `blog/`, `changelog/` and the docs folders
  are content collections (`src/content.config.ts`). `release.yml` reads
  `content/changelog/*.mdx` by their `version:` line, so keep that path.
- **Docs navigation is `src/docs-nav.ts`.** A new page needs an entry there;
  the build fails when a file and the nav disagree.
- **Mermaid is drawn at build time** by `rehype-mermaid` in Playwright's
  Chromium (`playwright install chromium` once locally).
- **The roadmap is fetched from GitHub Projects at build time**
  (`src/lib/roadmap.ts`); a build without network still succeeds with a
  fallback. It is as fresh as the last deploy.
- **Benchmark numbers** in `src/lib/compare.ts` and `compare-series.json` are
  copied from `packages/benchmarks/results/compare/`; never edit one by hand.
- **Copy:** no em or en dashes in visible text or titles, and no filler
  headlines.
- `public/openapi.json` is copied from `apps/server/openapi.json` by `dev` and
  `build`; it is not committed.
