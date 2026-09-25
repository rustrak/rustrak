export default {
  /*
    The landing, which no longer lives in `content/` at all — it is
    `app/page.tsx`, outside the docs layout entirely.

    It still needs an entry here, and the reason is easy to miss: Nextra builds
    its page map from the `app/` directory as well as from `content/`. That is
    how `/api-reference`, `/blog` and `/changelog` appear in the navigation
    without being MDX, and it means `app/page.tsx` gets discovered the same way.
    Left unconfigured it turns up at the top of the docs sidebar as a link
    titled with whatever the route's `metadata.title` says.

    So `display: 'hidden'` stays. What went, and stays gone, is the block of
    seven theme overrides that used to sit alongside it — `layout: 'full'`,
    `sidebar: false`, `toc: false` and the rest. Those were the landing opting
    out of a shell it is no longer inside.
  */
  index: {
    title: 'Home',
    type: 'page',
    display: 'hidden',
  },
  documentation: {
    title: 'Documentation',
    type: 'page',
    href: '/getting-started/overview',
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
    },
  },
};
