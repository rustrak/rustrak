import type { PluginSummary, ProjectResponse } from '@rustrak/client';
import {
  Button,
  confirm,
  FactList,
  Panel,
  SettingRow,
  SettingSection,
  SettingsHeader,
  Tab,
  TabList,
  Tabs,
} from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import { useLocale, useTranslations } from 'use-intl';
import { useRouter } from '@/shared/ui/hooks/use-router';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';
import { pluginMutations } from '../../api/mutations';
import { readmeIn } from '../../model/plugins';
import { Verification, VersionLine } from './plugin-catalog';
import { PluginMark } from './plugin-icon';
import { PluginProjects } from './plugin-projects';
import { PluginReadme } from './plugin-readme';
import { PluginSettingsForm } from './plugin-settings-form';

export type PluginTab = 'about' | 'settings';

/**
 * One plugin's page in Settings › Plugins: where it runs, its own setup page,
 * what it may do, what it is, and removing it.
 */
export function PluginDetail({
  plugin,
  projects,
  canManage,
  tab,
  onTab,
}: {
  plugin: PluginSummary;
  projects: ProjectResponse[];
  canManage: boolean;
  tab: PluginTab;
  onTab: (tab: PluginTab) => void;
}) {
  const t = useTranslations('settings');
  const locale = useLocale();
  return (
    <>
      <SettingsHeader
        leading={<PluginMark logo={plugin.logo} icon={plugin.icon} size="lg" />}
        title={plugin.name}
        description={
          <>
            <VersionLine plugin={plugin} />
            <Verification verified={plugin.verified} />
          </>
        }
        tabs={
          <Tabs value={tab} onValueChange={(next) => onTab(next as PluginTab)}>
            <TabList>
              <Tab value="about">{t('plugins.detail.about')}</Tab>
              <Tab value="settings">{t('plugins.detail.configuration')}</Tab>
            </TabList>
          </Tabs>
        }
      />
      {tab === 'about' ? (
        <div className="grid items-start gap-4 lg:grid-cols-[minmax(0,1fr)_300px]">
          <section className="min-w-0 rounded-2xl border border-border-subtle bg-surface">
            <PluginReadme markdown={readmeIn(plugin.readme, locale)} />
          </section>
          <AboutRail plugin={plugin} projects={projects.length} />
        </div>
      ) : (
        <>
          <PluginProjects
            plugin={plugin}
            projects={projects}
            canManage={canManage}
          />
          <SettingSection
            title={t('plugins.detail.defaults')}
            description={t('plugins.detail.defaultsHint')}
          >
            <PluginSettingsForm
              key={plugin.id}
              plugin={plugin}
              canEdit={canManage}
            />
          </SettingSection>
          {canManage ? <DangerSection plugin={plugin} /> : null}
        </>
      )}
    </>
  );
}

/** Beside the README: what the plugin is, where it runs, what it may do. */
function AboutRail({
  plugin,
  projects,
}: {
  plugin: PluginSummary;
  projects: number;
}) {
  const t = useTranslations('settings');
  const none = t('plugins.detail.none');
  const pages = plugin.ui?.pages.length ?? 0;
  const widgets = plugin.ui?.widgets.length ?? 0;
  return (
    <div className="flex flex-col gap-4">
      <Panel title={t('plugins.detail.details')}>
        <FactList
          className="gap-y-2"
          valueClassName="text-fg-secondary"
          facts={[
            [t('plugins.detail.version'), plugin.version],
            [t('plugins.detail.author'), plugin.author ?? none],
            [
              t('plugins.detail.source'),
              plugin.verified
                ? t('plugins.detail.verified')
                : t('plugins.detail.unverified'),
            ],
            [
              t('plugins.detail.runsIn'),
              plugin.allProjects
                ? t('plugins.detail.everyProject')
                : t('plugins.detail.someProjects', {
                    count: plugin.projects.length,
                    total: projects,
                  }),
            ],
            [t('plugins.detail.pages'), String(pages)],
            [t('plugins.detail.widgets'), String(widgets)],
            [
              t('plugins.detail.repository'),
              plugin.repository ? (
                <a
                  href={plugin.repository}
                  target="_blank"
                  rel="noreferrer"
                  className="truncate text-fg-brand"
                >
                  {plugin.repository.replace(/^https?:\/\//, '')}
                </a>
              ) : (
                none
              ),
            ],
          ]}
        />
      </Panel>
      <Panel
        title={t('plugins.detail.access')}
        hint={t('plugins.detail.accessHint')}
      >
        <FactList
          className="gap-y-2"
          valueClassName="text-fg-secondary"
          facts={[
            [
              t('plugins.detail.database'),
              plugin.database ? t('plugins.detail.yes') : none,
            ],
            [
              t('plugins.detail.permissions'),
              plugin.permissions
                .map((p) => `${p.resource} (${p.actions.join(', ')})`)
                .join(' · ') || none,
            ],
          ]}
        />
      </Panel>
    </div>
  );
}

function DangerSection({ plugin }: { plugin: PluginSummary }) {
  const t = useTranslations('settings');
  const toastFailure = useToastFailure();
  const router = useRouter();
  const uninstall = useMutation(pluginMutations.uninstall(plugin.id));

  const remove = async (purge: boolean) => {
    const ok = await confirm({
      title: t('plugins.detail.uninstallTitle', { name: plugin.name }),
      description: purge
        ? t('plugins.detail.purgeConfirm')
        : t('plugins.detail.keepConfirm'),
      tone: 'danger',
      confirmLabel: purge
        ? t('plugins.detail.uninstallPurge')
        : t('plugins.detail.uninstall'),
      cancelLabel: t('plugins.detail.cancel'),
    });
    if (!ok) return;
    uninstall.mutate(purge, {
      onSuccess: () => {
        toast.success(t('plugins.detail.uninstalled', { name: plugin.name }));
        router.push('/settings/plugins');
      },
      onError: (error) =>
        toastFailure(t('plugins.detail.uninstallFailed'), error),
    });
  };

  return (
    <SettingSection title={t('plugins.detail.danger')} destructive>
      <SettingRow
        title={t('plugins.detail.uninstall')}
        description={t('plugins.detail.uninstallHint')}
      >
        <div className="flex flex-wrap gap-2">
          <Button
            variant="secondary"
            disabled={uninstall.isPending}
            onClick={() => void remove(false)}
          >
            {t('plugins.detail.uninstall')}
          </Button>
          <Button
            variant="danger"
            disabled={uninstall.isPending}
            onClick={() => void remove(true)}
          >
            {t('plugins.detail.uninstallPurge')}
          </Button>
        </div>
      </SettingRow>
    </SettingSection>
  );
}
