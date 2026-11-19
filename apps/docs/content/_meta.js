export default {
  /*
    The page map is built from `content/` only (patches/nextra@4.6.1.patch).
    Stock Nextra also imports every `page.tsx` under `app/` into it, which put the
    landing, the roadmap and the API reference in the JS of every docs page.
    So a route that lives in `app/` is listed here by `href`, like
    `documentation`, rather than discovered.
  */
  documentation: {
    title: 'Documentation',
    type: 'page',
    href: '/getting-started/overview',
  },
  // `app/roadmap`, a standalone route like the landing; listed so the docs
  // navbar links to it.
  roadmap: {
    title: 'Roadmap',
    type: 'page',
    href: '/roadmap',
  },
  changelog: {
    title: 'Changelog',
    type: 'page',
    theme: {
      sidebar: false,
      toc: false,
      breadcrumb: false,
      pagination: false,
    },
  },
  blog: {
    title: 'Blog',
    type: 'page',
    theme: {
      sidebar: false,
      toc: false,
      breadcrumb: false,
      pagination: false,
    },
  },
  // `app/benchmarks`, a standalone route like the roadmap, listed so the docs
  // navbar links to it. `app/vs/[slug]` needs no entry: Nextra leaves dynamic
  // routes out of the page map.
  benchmarks: {
    title: 'Benchmarks',
    type: 'page',
    href: '/benchmarks',
  },
  'getting-started': 'Getting Started',
  configuration: 'Configuration',
  upgrading: 'Upgrading',
  usage: 'Usage',
  sdks: 'SDKs & Integrations',
  troubleshooting: 'Troubleshooting',
  '---': {
    type: 'separator',
  },
  reference: {
    title: 'Reference',
    type: 'menu',
    items: {
      'api-reference': { title: 'API Reference', href: '/api-reference' },
      architecture: { title: 'Architecture', href: '/reference/architecture' },
      contributing: { title: 'Contributing', href: '/reference/contributing' },
      llms: { title: 'Docs for AI assistants', href: '/reference/llms' },
    },
  },
};
