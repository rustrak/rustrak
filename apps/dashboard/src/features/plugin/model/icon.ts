import { type IconComponent, icons, PluginsIcon } from '@rustrak/ui';

/** A plugin's icon by its `@rustrak/ui` name; the puzzle piece when it named none. */
export function pluginIcon(name?: string | null): IconComponent {
  return (
    (name && Object.hasOwn(icons, name)
      ? (icons as Record<string, IconComponent>)[name]
      : undefined) ?? PluginsIcon
  );
}
