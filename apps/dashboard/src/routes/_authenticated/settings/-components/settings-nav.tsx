import { Link } from '@tanstack/react-router';
import { Database, Info, Key, Palette, Plug, User, Users } from 'lucide-react';
import { useTranslations } from 'use-intl';

interface NavItem {
  to:
    | '/settings/tokens'
    | '/settings/integrations'
    | '/settings/team'
    | '/settings/storage'
    | '/settings/account'
    | '/settings/appearance'
    | '/settings/about';
  labelKey: string;
  icon: React.ElementType;
  adminOnly?: boolean;
}

const navItems: NavItem[] = [
  { to: '/settings/tokens', labelKey: 'nav.apiTokens', icon: Key },
  { to: '/settings/integrations', labelKey: 'nav.integrations', icon: Plug },
  {
    to: '/settings/team',
    labelKey: 'nav.team',
    icon: Users,
    adminOnly: true,
  },
  {
    to: '/settings/storage',
    labelKey: 'nav.storage',
    icon: Database,
    adminOnly: true,
  },
  { to: '/settings/account', labelKey: 'nav.account', icon: User },
  { to: '/settings/appearance', labelKey: 'nav.appearance', icon: Palette },
  { to: '/settings/about', labelKey: 'nav.about', icon: Info },
];

/** Only the page itself is active, whatever its query string holds. */
const NAV_ACTIVE = { exact: true, includeSearch: false };

interface SettingsNavProps {
  onNavigate?: () => void;
  isAdmin?: boolean;
}

export function SettingsNav({ onNavigate, isAdmin = false }: SettingsNavProps) {
  const t = useTranslations('settings');
  const visibleItems = navItems.filter((item) => !item.adminOnly || isAdmin);

  return (
    <nav className="flex flex-col gap-1">
      {visibleItems.map((item) => {
        const Icon = item.icon;

        return (
          <Link
            key={item.to}
            to={item.to}
            onClick={onNavigate}
            className="flex items-center gap-3 px-3 py-2.5 rounded-md text-sm transition-colors"
            activeOptions={NAV_ACTIVE}
            activeProps={{
              className: 'bg-primary text-primary-foreground font-bold',
            }}
            inactiveProps={{
              className:
                'font-medium text-muted-foreground hover:bg-accent hover:text-accent-foreground',
            }}
          >
            <Icon className="size-4" />
            {t(item.labelKey)}
          </Link>
        );
      })}
    </nav>
  );
}
