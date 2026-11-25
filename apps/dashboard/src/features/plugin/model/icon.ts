import { type IconComponent, icons, PluginsIcon } from '@rustrak/ui';

/** A plugin's icon by its `@rustrak/ui` name; the puzzle piece when it named none. */
export function pluginIcon(name?: string | null): IconComponent {
  return (
    (name && (icons as Record<string, IconComponent>)[name]) || PluginsIcon
  );
}
