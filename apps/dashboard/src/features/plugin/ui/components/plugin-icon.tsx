import { cn } from '@rustrak/ui';
import { useState } from 'react';
import { pluginIcon } from '../../model/icon';

const MARK = {
  sm: { box: 'size-8 rounded-lg', icon: 'sm' },
  md: { box: 'size-10 rounded-xl', icon: 'md' },
  lg: { box: 'size-12 rounded-xl', icon: 'lg' },
} as const;

/**
 * A plugin's mark: its own `logo.svg` when it ships one, drawn as an image
 * (an SVG's scripts never run there), else its design-system icon. The icon
 * holds the place until the logo has loaded, and stays if it never does.
 */
export function PluginMark({
  logo,
  icon,
  size = 'md',
}: {
  logo?: string | null;
  icon?: string | null;
  size?: keyof typeof MARK;
}) {
  const mark = MARK[size];
  const [loaded, setLoaded] = useState<string | null>(null);
  const Icon = pluginIcon(icon);
  const shown = !!logo && loaded === logo;
  return (
    <span
      className={cn(
        mark.box,
        'relative flex shrink-0 items-center justify-center overflow-hidden border border-border-subtle text-fg-secondary',
      )}
    >
      {shown ? null : <Icon size={mark.icon} />}
      {logo ? (
        <img
          src={logo}
          alt=""
          onLoad={() => setLoaded(logo)}
          className={cn(
            'absolute inset-0 size-full object-contain',
            !shown && 'opacity-0',
          )}
          data-testid="plugin-logo"
        />
      ) : null}
    </span>
  );
}
