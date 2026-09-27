import { Link } from '@tanstack/react-router';
import { Bell, KeyRound, SlidersHorizontal, Users } from 'lucide-react';
import { useTranslations } from 'use-intl';

type ProjectSettingsPath =
  | '/projects/$id/settings/general'
  | '/projects/$id/settings/alerts'
  | '/projects/$id/settings/members'
  | '/projects/$id/settings/client-keys';

interface NavGroup {
  /** Rendered above the group. Omit for a single ungrouped list. */
  labelKey?: string;
  items: {
    to: ProjectSettingsPath;
    labelKey: string;
    icon: React.ElementType;
  }[];
}

/**
 * Grouped so this keeps working as project settings grow, mirroring how Sentry
 * splits the equivalent nav (General / Processing / SDK Setup). Later additions
 * become new entries or new groups here rather than a longer flat list.
 */
const navGroups: NavGroup[] = [
  {
    labelKey: 'nav.groupProject',
    items: [
      {
        to: '/projects/$id/settings/general',
        labelKey: 'nav.general',
        icon: SlidersHorizontal,
      },
      {
        to: '/projects/$id/settings/alerts',
        labelKey: 'nav.alerts',
        icon: Bell,
      },
      {
        to: '/projects/$id/settings/members',
        labelKey: 'nav.members',
        icon: Users,
      },
    ],
  },
  {
    labelKey: 'nav.groupSdkSetup',
    items: [
      {
        to: '/projects/$id/settings/client-keys',
        labelKey: 'nav.clientKeys',
        icon: KeyRound,
      },
    ],
  },
];

/** Only the page itself is active, whatever its query string holds. */
const NAV_ACTIVE = { exact: true, includeSearch: false };

interface ProjectSettingsNavProps {
  projectId: number;
  onNavigate?: () => void;
}

export function ProjectSettingsNav({
  projectId,
  onNavigate,
}: ProjectSettingsNavProps) {
  const t = useTranslations('settings');
  const showLabels = navGroups.length > 1;

  return (
    <nav className="flex flex-col gap-6">
      {navGroups.map((group, index) => (
        // `navGroups` is a module-level constant. The index is the fallback for
        // the one group that carries no label, and it cannot collide because a
        // label is either present and unique or absent exactly once.
        // react-doctor-disable-next-line react-doctor/no-array-index-as-key
        <div key={group.labelKey ?? index} className="flex flex-col gap-1">
          {showLabels && group.labelKey && (
            <span className="mb-1 px-3 text-xs font-bold uppercase tracking-widest text-muted-foreground">
              {t(group.labelKey)}
            </span>
          )}
          {group.items.map((item) => {
            const Icon = item.icon;

            return (
              <Link
                key={item.to}
                to={item.to}
                params={{ id: projectId }}
                onClick={onNavigate}
                className="flex items-center gap-3 rounded-md px-3 py-2.5 text-sm transition-colors"
                activeOptions={NAV_ACTIVE}
                activeProps={{
                  className: 'bg-primary font-bold text-primary-foreground',
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
        </div>
      ))}
    </nav>
  );
}
