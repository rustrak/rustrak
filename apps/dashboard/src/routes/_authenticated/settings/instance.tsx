import { noop, useQuery } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { useFormatter, useTranslations } from 'use-intl';
import { projectQueries } from '@/features/project/api/queries';
import { serverVersionQuery } from '@/shared/api/server-version';
import { APP_VERSION } from '@/shared/config/constants';
import { describeError } from '@/shared/lib/error-copy';
import { SettingRow, SettingSection } from '@/shared/ui/components/setting-row';
import { SettingsHeader } from '@/shared/ui/components/settings-header';

export const Route = createFileRoute('/_authenticated/settings/instance')({
  // Both are rows the page paints without: it never suspends on them.
  loader: ({ context: { queryClient } }) => {
    void queryClient.query(serverVersionQuery()).catch(noop);
    void queryClient.query(projectQueries.rateLimits()).catch(noop);
  },
  component: InstancePage,
});

const LINKS = [
  { key: 'githubRepository', href: 'https://github.com/rustrak/rustrak' },
  { key: 'reportAnIssue', href: 'https://github.com/rustrak/rustrak/issues' },
  { key: 'sentryDocs', href: 'https://docs.sentry.io/platforms/' },
] as const;

/**
 * What this Rustrak is: the versions it runs, and the limits it applies to
 * every project. It is what someone needs in front of them when they report
 * a bug or wonder why a burst of events was refused.
 */
function InstancePage() {
  const t = useTranslations('settings');
  const tProjects = useTranslations('projects');
  const rootT = useTranslations();
  const format = useFormatter();
  const serverVersion = useQuery(serverVersionQuery());
  const limits = useQuery(projectQueries.rateLimits());

  const limit = (value: number) =>
    value > 0 ? format.number(value) : t('instance.unlimited');

  return (
    <>
      <title>{`${t('instance.title')} | Rustrak`}</title>
      <SettingsHeader
        title={t('instance.title')}
        description={t('instance.subtitle')}
      />

      <SettingSection title={t('instance.version')}>
        <SettingRow title={t('about.serverVersion')}>
          <p className="font-mono text-mono text-fg">
            {serverVersion.isError
              ? t('about.couldNotRead')
              : serverVersion.data &&
                (serverVersion.data.version
                  ? `v${serverVersion.data.version}`
                  : t('about.notReported'))}
          </p>
        </SettingRow>
        <SettingRow title={t('about.webviewVersion')}>
          <p className="font-mono text-mono text-fg">v{APP_VERSION}</p>
        </SettingRow>
        <SettingRow title={t('about.environment')}>
          <p className="font-mono text-mono text-fg">{import.meta.env.MODE}</p>
        </SettingRow>
        {serverVersion.isError && (
          <p className="px-5 py-3 text-hint text-fg-subtle">
            {describeError(serverVersion.error.error, rootT)}{' '}
            {t('about.serverVersionUnknown')}
          </p>
        )}
      </SettingSection>

      {limits.data ? (
        <SettingSection
          title={t('instance.limits')}
          description={t('instance.limitsDescription')}
        >
          <SettingRow title={tProjects('rateLimits.perMinute')}>
            <p className="text-value text-fg tabular-nums">
              {limit(limits.data.project_per_minute)}
            </p>
          </SettingRow>
          <SettingRow title={tProjects('rateLimits.perHour')}>
            <p className="text-value text-fg tabular-nums">
              {limit(limits.data.project_per_hour)}
            </p>
          </SettingRow>
        </SettingSection>
      ) : null}

      <SettingSection title={t('about.links')}>
        {LINKS.map((link) => (
          <SettingRow key={link.key} title={t(`about.${link.key}`)}>
            <a
              href={link.href}
              target="_blank"
              rel="noopener noreferrer"
              className="truncate text-value text-fg-brand hover:underline"
            >
              {link.href.replace('https://', '')}
            </a>
          </SettingRow>
        ))}
      </SettingSection>
    </>
  );
}
