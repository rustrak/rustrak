import { cn } from '@rustrak/ui';
import { pluginIcon } from '../../model/icon';

const MARK = {
  sm: { box: 'size-8 rounded-lg', icon: 'sm' },
  md: { box: 'size-10 rounded-xl', icon: 'md' },
  lg: { box: 'size-12 rounded-xl', icon: 'lg' },
} as const;

/**
 * A plugin's mark: its own `logo.svg` when it ships one, drawn as an image
 * (an SVG's scripts never run there), else its design-system icon.
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
  if (logo) {
    return (
      <img
        src={logo}
        alt=""
        className={cn(
          mark.box,
          'shrink-0 border border-border-subtle object-contain',
        )}
        data-testid="plugin-logo"
      />
    );
  }
  const Icon = pluginIcon(icon);
  return (
    <span
      className={cn(
        mark.box,
        'flex shrink-0 items-center justify-center border border-border-subtle text-fg-secondary',
      )}
    >
      <Icon size={mark.icon} />
    </span>
  );
}
