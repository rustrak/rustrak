/**
 * Every platform a user may pick for a project, mirroring
 * `SELECTABLE_PLATFORMS` in `apps/server/src/models/project.rs`. Keep both in
 * sync: the server rejects anything not on its list.
 *
 * Generated from Sentry's own `static/app/data/platforms.tsx` (@a32a33a5)
 * plus the platform ids Relay's `VALID_PLATFORMS` carries that Sentry omits
 * from that file, so anything auto-detection can write stays selectable.
 *
 * Note this is NOT the list of valid *event* platforms. Those are far
 * narrower: Relay rewrites any event platform outside its 19 to "other".
 */
export interface Platform {
  id: string;
  name: string;
  /** Groups frameworks under their base language in the picker. */
  language: string;
}

export const PLATFORMS: readonly Platform[] = [
  { id: 'android', name: 'Android', language: 'android' },
  { id: 'apple', name: 'Apple', language: 'apple' },
  { id: 'apple-ios', name: 'iOS', language: 'apple' },
  { id: 'apple-macos', name: 'macOS', language: 'apple' },
  { id: 'as3', name: 'ActionScript', language: 'as3' },
  { id: 'bun', name: 'Bun', language: 'bun' },
  { id: 'c', name: 'C', language: 'c' },
  { id: 'capacitor', name: 'Capacitor', language: 'capacitor' },
  { id: 'cfml', name: 'ColdFusion', language: 'cfml' },
  { id: 'cocoa', name: 'Cocoa', language: 'cocoa' },
  { id: 'cordova', name: 'Cordova', language: 'cordova' },
  { id: 'csharp', name: 'C#', language: 'csharp' },
  { id: 'dart', name: 'Dart', language: 'dart' },
  { id: 'deno', name: 'Deno', language: 'deno' },
  { id: 'dotnet', name: '.NET', language: 'dotnet' },
  { id: 'dotnet-aspnet', name: 'ASP.NET', language: 'dotnet' },
  { id: 'dotnet-aspnetcore', name: 'ASP.NET Core', language: 'dotnet' },
  { id: 'dotnet-awslambda', name: 'AWS Lambda (.NET)', language: 'dotnet' },
  {
    id: 'dotnet-gcpfunctions',
    name: 'Google Cloud Functions (.NET)',
    language: 'dotnet',
  },
  { id: 'dotnet-maui', name: '.NET MAUI', language: 'dotnet' },
  { id: 'dotnet-uwp', name: 'UWP', language: 'dotnet' },
  { id: 'dotnet-winforms', name: 'Windows Forms', language: 'dotnet' },
  { id: 'dotnet-wpf', name: 'WPF', language: 'dotnet' },
  { id: 'dotnet-xamarin', name: 'Xamarin', language: 'dotnet' },
  { id: 'electron', name: 'Electron', language: 'electron' },
  { id: 'elixir', name: 'Elixir', language: 'elixir' },
  { id: 'flutter', name: 'Flutter', language: 'flutter' },
  { id: 'go', name: 'Go', language: 'go' },
  { id: 'go-echo', name: 'Echo', language: 'go' },
  { id: 'go-fasthttp', name: 'FastHTTP', language: 'go' },
  { id: 'go-fiber', name: 'Fiber', language: 'go' },
  { id: 'go-gin', name: 'Gin', language: 'go' },
  { id: 'go-http', name: 'Net/Http', language: 'go' },
  { id: 'go-iris', name: 'Iris', language: 'go' },
  { id: 'go-martini', name: 'Martini', language: 'go' },
  { id: 'go-negroni', name: 'Negroni', language: 'go' },
  { id: 'godot', name: 'Godot', language: 'godot' },
  { id: 'groovy', name: 'Groovy', language: 'groovy' },
  { id: 'haskell', name: 'Haskell', language: 'haskell' },
  { id: 'ionic', name: 'Ionic', language: 'ionic' },
  { id: 'java', name: 'Java', language: 'java' },
  { id: 'java-log4j2', name: 'Log4j 2.x', language: 'java' },
  { id: 'java-logback', name: 'Logback', language: 'java' },
  { id: 'java-spring', name: 'Spring', language: 'java' },
  { id: 'java-spring-boot', name: 'Spring Boot', language: 'java' },
  { id: 'javascript', name: 'Browser JavaScript', language: 'javascript' },
  { id: 'javascript-angular', name: 'Angular', language: 'javascript' },
  { id: 'javascript-astro', name: 'Astro', language: 'javascript' },
  { id: 'javascript-ember', name: 'Ember', language: 'javascript' },
  { id: 'javascript-gatsby', name: 'Gatsby', language: 'javascript' },
  { id: 'javascript-nextjs', name: 'Next.js', language: 'javascript' },
  { id: 'javascript-nuxt', name: 'Nuxt', language: 'javascript' },
  { id: 'javascript-react', name: 'React', language: 'javascript' },
  {
    id: 'javascript-react-router',
    name: 'React Router Framework',
    language: 'javascript',
  },
  { id: 'javascript-remix', name: 'Remix', language: 'javascript' },
  { id: 'javascript-solid', name: 'Solid', language: 'javascript' },
  { id: 'javascript-solidstart', name: 'SolidStart', language: 'javascript' },
  { id: 'javascript-svelte', name: 'Svelte', language: 'javascript' },
  { id: 'javascript-sveltekit', name: 'SvelteKit', language: 'javascript' },
  {
    id: 'javascript-tanstackstart-react',
    name: 'TanStack Start React',
    language: 'javascript',
  },
  { id: 'javascript-vue', name: 'Vue', language: 'javascript' },
  { id: 'kotlin', name: 'Kotlin', language: 'kotlin' },
  { id: 'minidump', name: 'Minidump', language: 'minidump' },
  { id: 'native', name: 'Native', language: 'native' },
  { id: 'native-qt', name: 'Qt', language: 'native' },
  { id: 'nintendo-switch', name: 'Nintendo Switch', language: 'console' },
  { id: 'node', name: 'Node.js', language: 'node' },
  { id: 'node-awslambda', name: 'AWS Lambda (Node)', language: 'node' },
  {
    id: 'node-azurefunctions',
    name: 'Azure Functions (Node)',
    language: 'node',
  },
  { id: 'node-cloudflare-pages', name: 'Cloudflare Pages', language: 'node' },
  {
    id: 'node-cloudflare-workers',
    name: 'Cloudflare Workers',
    language: 'node',
  },
  { id: 'node-connect', name: 'Connect', language: 'node' },
  { id: 'node-express', name: 'Express', language: 'node' },
  { id: 'node-fastify', name: 'Fastify', language: 'node' },
  {
    id: 'node-gcpfunctions',
    name: 'Google Cloud Functions (Node)',
    language: 'node',
  },
  { id: 'node-hapi', name: 'Hapi', language: 'node' },
  { id: 'node-hono', name: 'Hono', language: 'node' },
  { id: 'node-koa', name: 'Koa', language: 'node' },
  { id: 'node-nestjs', name: 'Nest.js', language: 'node' },
  { id: 'objc', name: 'Objective-C', language: 'objc' },
  { id: 'other', name: 'Other', language: 'other' },
  { id: 'perl', name: 'Perl', language: 'perl' },
  { id: 'php', name: 'PHP', language: 'php' },
  { id: 'php-laravel', name: 'Laravel', language: 'php' },
  { id: 'php-symfony', name: 'Symfony', language: 'php' },
  { id: 'playstation', name: 'PlayStation', language: 'console' },
  { id: 'powershell', name: 'PowerShell', language: 'powershell' },
  { id: 'python', name: 'Python', language: 'python' },
  { id: 'python-aiohttp', name: 'AIOHTTP', language: 'python' },
  { id: 'python-asgi', name: 'ASGI', language: 'python' },
  { id: 'python-awslambda', name: 'AWS Lambda (Python)', language: 'python' },
  { id: 'python-bottle', name: 'Bottle', language: 'python' },
  { id: 'python-celery', name: 'Celery', language: 'python' },
  { id: 'python-chalice', name: 'Chalice', language: 'python' },
  { id: 'python-django', name: 'Django', language: 'python' },
  { id: 'python-falcon', name: 'Falcon', language: 'python' },
  { id: 'python-fastapi', name: 'FastAPI', language: 'python' },
  { id: 'python-flask', name: 'Flask', language: 'python' },
  {
    id: 'python-gcpfunctions',
    name: 'Google Cloud Functions (Python)',
    language: 'python',
  },
  { id: 'python-litestar', name: 'Litestar', language: 'python' },
  { id: 'python-pylons', name: 'Pylons', language: 'python' },
  { id: 'python-pymongo', name: 'PyMongo', language: 'python' },
  { id: 'python-pyramid', name: 'Pyramid', language: 'python' },
  { id: 'python-quart', name: 'Quart', language: 'python' },
  { id: 'python-rq', name: 'RQ (Redis Queue)', language: 'python' },
  { id: 'python-sanic', name: 'Sanic', language: 'python' },
  { id: 'python-serverless', name: 'Serverless (Python)', language: 'python' },
  { id: 'python-starlette', name: 'Starlette', language: 'python' },
  { id: 'python-tornado', name: 'Tornado', language: 'python' },
  { id: 'python-tryton', name: 'Tryton', language: 'python' },
  { id: 'python-wsgi', name: 'WSGI', language: 'python' },
  { id: 'react-native', name: 'React Native', language: 'react-native' },
  { id: 'ruby', name: 'Ruby', language: 'ruby' },
  { id: 'ruby-rack', name: 'Rack Middleware', language: 'ruby' },
  { id: 'ruby-rails', name: 'Rails', language: 'ruby' },
  { id: 'rust', name: 'Rust', language: 'rust' },
  { id: 'unity', name: 'Unity', language: 'unity' },
  { id: 'unreal', name: 'Unreal Engine', language: 'unreal' },
  { id: 'xbox', name: 'Xbox', language: 'console' },
];

const LANGUAGE_LABELS: Record<string, string> = {
  as3: 'ActionScript',
  c: 'C',
  cfml: 'ColdFusion',
  cocoa: 'Cocoa',
  csharp: 'C#',
  groovy: 'Groovy',
  haskell: 'Haskell',
  objc: 'Objective-C',
  other: 'Other',
  perl: 'Perl',
  apple: 'Apple',
  android: 'Android',
  bun: 'Bun',
  capacitor: 'Capacitor',
  console: 'Consoles',
  cordova: 'Cordova',
  dart: 'Dart',
  deno: 'Deno',
  dotnet: '.NET',
  electron: 'Electron',
  elixir: 'Elixir',
  flutter: 'Flutter',
  go: 'Go',
  godot: 'Godot',
  ionic: 'Ionic',
  java: 'Java',
  javascript: 'JavaScript',
  kotlin: 'Kotlin',
  minidump: 'Minidump',
  native: 'Native',
  node: 'Node.js',
  php: 'PHP',
  powershell: 'PowerShell',
  python: 'Python',
  'react-native': 'React Native',
  ruby: 'Ruby',
  rust: 'Rust',
  unity: 'Unity',
  unreal: 'Unreal Engine',
};

/** Human-readable name of the group a platform belongs to. */
export function languageLabel(language: string): string {
  return LANGUAGE_LABELS[language] ?? language;
}

/** Display name for a platform id, falling back to the raw id. */
export function platformLabel(id: string): string {
  return PLATFORMS.find((p) => p.id === id)?.name ?? id;
}

// ---------------------------------------------------------------------------
// Picker categories
//
// Membership is copied verbatim from Sentry's own
// `static/app/data/platformPickerCategories.tsx` (@a32a33a5), which is a
// different file from `platformCategories.tsx` (that one gates per-product
// features and does NOT drive the picker).
//
// Two deliberate divergences from Sentry:
//
//  1. Sentry's "All" tab is `createablePlatforms`, the union of every category
//     EXCEPT gaming. Ours is genuinely every platform in PLATFORMS. Sentry can
//     afford a narrower All because its picker is not the only way to set a
//     platform; ours is, and SELECTABLE_PLATFORMS deliberately includes ids
//     auto-detection can write (`perl`, `cfml`, `as3`, ...). Hiding those would
//     recreate the exact upstream bug where a detected platform cannot be
//     re-selected.
//  2. Sentry appends gaming platforms separately and strips consoles when
//     self-hosted. Rustrak is always self-hosted and has no console
//     entitlements to check, so gaming is a plain category.
// ---------------------------------------------------------------------------

export interface PlatformCategory {
  /** Also its label's key: `platforms.categories.<id>`. */
  id: string;
  /**
   * Platform ids in this category. Ids not present in PLATFORMS are ignored by
   * `categoryPlatforms`, so this list can never offer something the server
   * would reject.
   *
   * Omitted means "every platform", which is what the `all` tab uses.
   */
  platforms?: readonly string[];
}

/**
 * Ordered as rendered. `popular` is first and is the default tab.
 *
 * Its order is hand-curated by Sentry and is preserved as authored rather than
 * sorted, unlike every other category.
 */
export const PLATFORM_CATEGORIES: readonly PlatformCategory[] = [
  {
    id: 'popular',
    platforms: [
      'javascript-nextjs',
      'javascript-react',
      'react-native',
      'node',
      'php-laravel',
      'python-fastapi',
      'flutter',
      'python-django',
      'python',
      'node-express',
      'javascript',
      'php',
      'ruby-rails',
      'apple-ios',
      'node-nestjs',
      'python-flask',
      'javascript-vue',
      'dotnet-aspnetcore',
      'javascript-nuxt',
      'dotnet-maui',
      'javascript-angular',
      'android',
      'java-spring-boot',
      'php-symfony',
      'node-cloudflare-workers',
      'electron',
      'unity',
      'javascript-remix',
    ],
  },
  {
    id: 'browser',
    platforms: [
      'dart',
      'flutter',
      'javascript',
      'javascript-angular',
      'javascript-astro',
      'javascript-ember',
      'javascript-gatsby',
      'javascript-nextjs',
      'javascript-nuxt',
      'javascript-react',
      'javascript-react-router',
      'javascript-remix',
      'javascript-solid',
      'javascript-solidstart',
      'javascript-svelte',
      'javascript-sveltekit',
      'javascript-tanstackstart-react',
      'javascript-vue',
      'react-native',
      'unity',
    ],
  },
  {
    id: 'server',
    platforms: [
      'bun',
      'dart',
      'deno',
      'dotnet',
      'dotnet-aspnet',
      'dotnet-aspnetcore',
      'elixir',
      'go',
      'go-echo',
      'go-fasthttp',
      'go-fiber',
      'go-gin',
      'go-http',
      'go-iris',
      'go-negroni',
      'java',
      'java-log4j2',
      'java-logback',
      'java-spring',
      'java-spring-boot',
      'kotlin',
      'native',
      'node',
      'node-cloudflare-pages',
      'node-cloudflare-workers',
      'node-connect',
      'node-express',
      'node-fastify',
      'node-hapi',
      'node-hono',
      'node-koa',
      'node-nestjs',
      'php',
      'php-laravel',
      'php-symfony',
      'powershell',
      'python',
      'python-aiohttp',
      'python-asgi',
      'python-bottle',
      'python-celery',
      'python-chalice',
      'python-django',
      'python-falcon',
      'python-fastapi',
      'python-flask',
      'python-litestar',
      'python-pyramid',
      'python-quart',
      'python-rq',
      'python-sanic',
      'python-starlette',
      'python-tornado',
      'python-tryton',
      'python-wsgi',
      'ruby',
      'ruby-rack',
      'ruby-rails',
      'rust',
    ],
  },
  {
    id: 'mobile',
    platforms: [
      'android',
      'apple-ios',
      'capacitor',
      'cordova',
      'dart',
      'dotnet-maui',
      'dotnet-xamarin',
      'flutter',
      'ionic',
      'react-native',
      'unity',
      'unreal',
    ],
  },
  {
    id: 'desktop',
    platforms: [
      'apple-macos',
      'dart',
      'dotnet',
      'dotnet-maui',
      'dotnet-winforms',
      'dotnet-wpf',
      'electron',
      'flutter',
      'godot',
      'java',
      'kotlin',
      'minidump',
      'native',
      'native-qt',
      'unity',
      'unreal',
    ],
  },
  {
    id: 'serverless',
    platforms: [
      'dotnet-awslambda',
      'dotnet-gcpfunctions',
      'node-awslambda',
      'node-azurefunctions',
      'node-cloudflare-pages',
      'node-cloudflare-workers',
      'node-gcpfunctions',
      'python-awslambda',
      'python-gcpfunctions',
      'python-serverless',
    ],
  },
  {
    id: 'gaming',
    platforms: [
      'godot',
      'native',
      'nintendo-switch',
      'playstation',
      'unity',
      'unreal',
      'xbox',
    ],
  },
  {
    id: 'all',
    // No list: resolved from PLATFORMS so it can never fall behind.
  },
];

/**
 * Extra search terms per platform, so a user typing what they call the thing
 * finds it. Sentry defines exactly one.
 */
const PLATFORM_SEARCH_ALIASES: Record<string, string[]> = {
  native: ['cpp', 'c++'],
};

const PLATFORMS_BY_ID = new Map(PLATFORMS.map((p) => [p.id, p]));

/**
 * Platforms in a category, resolved against PLATFORMS.
 *
 * `popular` keeps its curated order; every other category is sorted by name,
 * matching Sentry. `all` is every known platform.
 */
export function categoryPlatforms(categoryId: string): Platform[] {
  const category = PLATFORM_CATEGORIES.find((c) => c.id === categoryId);
  if (!category) return [];

  if (!category.platforms) {
    return [...PLATFORMS].sort((a, b) => a.name.localeCompare(b.name));
  }

  const resolved = category.platforms
    .map((id) => PLATFORMS_BY_ID.get(id))
    .filter((p): p is Platform => p !== undefined);

  return categoryId === 'popular'
    ? resolved
    : resolved.sort((a, b) => a.name.localeCompare(b.name));
}

/**
 * Free-text platform search.
 *
 * Deliberately ignores the active category, matching Sentry: a user who types
 * "django" while the Browser tab is open should still find it, rather than
 * being told there are no results.
 */
export function searchPlatforms(query: string): Platform[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];

  return PLATFORMS.filter(
    (p) =>
      p.id.includes(q) ||
      p.name.toLowerCase().includes(q) ||
      (PLATFORM_SEARCH_ALIASES[p.id]?.some((alias) => alias.includes(q)) ??
        false),
  ).sort((a, b) => a.name.localeCompare(b.name));
}
