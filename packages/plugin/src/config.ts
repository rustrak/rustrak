// `rustrak.config.ts`: what a plugin says about itself beyond its code. Typed
// so a mistake is an error in the editor, not at install: every choice is a
// union of what Rustrak accepts. Rustrak checks it again when it installs.

import { existsSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import type { icons } from '@rustrak/ui';

/** Where something of a plugin lives: once for the instance, or per project. */
export type Scope = 'instance' | 'project';

export type PermissionAction = 'read' | 'create' | 'update' | 'delete';

type NonEmpty<T> = readonly [T, ...T[]];

/** One permission people can be granted, in Roles and Members. */
export interface PermissionDecl {
  scope: Scope;
  actions: NonEmpty<PermissionAction>;
}

/** `permissions`: by resource, lowercase words and dashes. */
export type Permissions = Record<Lowercase<string>, PermissionDecl>;

interface SettingBase {
  /** Where it is set: once for the instance (default), or per project. */
  scope?: Scope;
  /** The plugin answers 503 until it is set. */
  required?: boolean;
}

export type TextSetting = SettingBase & {
  type: 'text' | 'url';
  default?: string;
};

/** Never sent to the browser; set once, for the instance. */
export type SecretSetting = Omit<SettingBase, 'scope'> & {
  type: 'secret';
  scope?: 'instance';
};

export type NumberSetting = SettingBase & {
  type: 'number';
  default?: number;
  min?: number;
  max?: number;
  integer?: boolean;
};

export type BooleanSetting = SettingBase & {
  type: 'boolean';
  default?: boolean;
};

export type SelectSetting<Option extends string = string> = SettingBase & {
  type: 'select';
  /** Lowercase words and dashes: `daily`, `per-project`. */
  options: NonEmpty<Option>;
  default?: Option;
};

export type SettingField =
  | TextSetting
  | SecretSetting
  | NumberSetting
  | BooleanSetting
  | SelectSetting;

/** `settings`: by key, camelCase, its name in code. */
export type Settings = Record<string, SettingField>;

/** A select's `default` must be one of its `options`. */
type Checked<S> = {
  [K in keyof S]: S[K] extends { type: 'select'; options: readonly (infer O)[] }
    ? SelectSetting<O & string>
    : S[K];
};

/** `>=0.34.0`, `^0.34.0`, `>=0.34.0, <0.40.0`. */
type Version = `${number}.${number}.${number}`;
type Comparator = `${'>=' | '>' | '<=' | '<' | '=' | '^' | '~'}${Version}`;
export type VersionRange = Comparator | `${Comparator}, ${Comparator}`;

export interface PluginConfig<
  S extends Settings = Settings,
  P extends Permissions = Permissions,
> {
  /** Defaults to the package name, without scope or `rustrak-plugin-` prefix. */
  id?: Lowercase<string>;
  /** Shown in the catalog and the dashboard. */
  name: string;
  /** The Rustrak versions it runs on. */
  rustrak: VersionRange;
  /** Its mark when it has no `logo.svg`: a `@rustrak/ui` icon name. */
  icon?: keyof typeof icons;
  /** Quota of the plugin's own database, in MB. Defaults to 512. */
  storageMb?: number;
  /**
   * What the admin can change, drawn by Rustrak as the plugin's settings
   * form. Labelled by `settings.<key>.label` (and `.description`) in
   * src/messages/; read with `Settings::get(ctx)` in Rust and
   * `usePluginSettings()` in React.
   */
  settings?: S & Checked<S>;
  /**
   * What people can be granted, beside Rustrak's own. Labelled by
   * `permissions.<resource>.label` in src/messages/; required by routes as
   * `Perm::<Resource>.read()`, asked in React with
   * `usePluginPermissions().can()`.
   */
  permissions?: P;
}

/** Types `rustrak.config.ts`. */
export function defineConfig<
  const S extends Settings = Record<never, SettingField>,
  const P extends Permissions = Record<never, PermissionDecl>,
>(config: PluginConfig<S, P>): PluginConfig<S, P> {
  return config;
}

/** Loads the project's `rustrak.config.ts`, on Node's own type stripping. */
export async function loadConfig(dir: string): Promise<PluginConfig> {
  const file = join(dir, 'rustrak.config.ts');
  if (!existsSync(file)) throw new Error(`No rustrak.config.ts in ${dir}`);
  const mod = (await import(pathToFileURL(file).href)) as {
    default?: PluginConfig;
  };
  if (!mod.default) throw new Error('rustrak.config.ts has no default export');
  return mod.default;
}

/** Where each part of a plugin project lives. */
export function paths(dir: string) {
  const src = join(dir, 'src');
  return {
    /** What `rustrak-plugin` writes from the rest; never committed. */
    generated: join(dir, '.rustrak'),
    server: join(src, 'server'),
    ui: join(src, 'ui'),
    messages: join(src, 'messages'),
    public: join(src, 'public'),
    readme: join(dir, 'readme'),
    migrations: join(dir, 'migrations'),
  };
}

/** Every file under `dir`, at any depth; none when it is absent. */
export function filesIn(dir: string): string[] {
  if (!existsSync(dir)) return [];
  return (readdirSync(dir, { recursive: true }) as string[])
    .map((name) => join(dir, name))
    .filter((path) => statSync(path).isFile());
}
