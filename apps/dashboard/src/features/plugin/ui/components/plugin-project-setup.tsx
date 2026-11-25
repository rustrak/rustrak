import type { PluginSummary, ProjectResponse } from '@rustrak/client';
import {
  SettingRow,
  SettingSection,
  SettingsEmpty,
  SettingsHeader,
  Switch,
} from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useTranslations } from 'use-intl';
import { ProjectLogo } from '@/shared/ui/components/projects-picker';
import { usePermissions } from '@/shared/ui/hooks/use-permissions';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';
import { pluginMutations } from '../../api/mutations';
import { activeIn, withProject } from '../../model/plugins';
import { usePluginWidgets } from '../hooks/use-plugin-widgets';
import { PluginMark } from './plugin-icon';
import { PluginSettingsForm } from './plugin-settings-form';
import { PluginWidgets } from './plugin-widgets';

/**
 * A plugin in one project: whether it runs there, how to set it up (the
 * plugin's own guide), and the project's own values of its settings.
 */
export function PluginProjectSetup({
  plugin,
  project,
  canManage,
}: {
  plugin: PluginSummary;
  project: ProjectResponse;
  canManage: boolean;
}) {
  const t = useTranslations('settings');
  const toastFailure = useToastFailure();
  const configure = useMutation(pluginMutations.configure(plugin.id));
  const canEdit = usePermissions().can('project', 'update', project.id);
  const active = activeIn(plugin, project.id);
  const guide =
    usePluginWidgets('plugin.setup', project.id, plugin.id).length > 0;

  const toggle = (on: boolean) => {
    configure.mutate(
      { allProjects: false, projects: withProject(plugin, project.id, on) },
      { onError: (error) => toastFailure(t('plugins.configureFailed'), error) },
    );
  };

  return (
    <>
      <SettingsHeader
        leading={<PluginMark logo={plugin.logo} icon={plugin.icon} size="lg" />}
        title={t('plugins.setup.title', {
          plugin: plugin.name,
          project: project.name,
        })}
        description={
          <span className="flex items-center gap-2">
            <ProjectLogo project={project} size={16} />
            {project.name}
          </span>
        }
      />
      <SettingSection title={t('plugins.setup.where')}>
        <SettingRow
          title={t('plugins.setup.runs')}
          description={
            plugin.allProjects
              ? t('plugins.setup.everywhere')
              : t('plugins.setup.runsHint')
          }
        >
          <div className="flex justify-end">
            <Switch
              checked={active}
              disabled={!canManage || configure.isPending || plugin.allProjects}
              aria-label={t('plugins.detail.runIn', { project: project.name })}
              onCheckedChange={toggle}
            />
          </div>
        </SettingRow>
      </SettingSection>
      {active && guide ? (
        <SettingSection
          title={t('plugins.setup.install')}
          description={t('plugins.setup.installHint')}
        >
          <div className="px-5 py-4">
            <PluginWidgets
              zone="plugin.setup"
              projectId={project.id}
              pluginId={plugin.id}
              props={{ projectId: project.id }}
            />
          </div>
        </SettingSection>
      ) : null}
      <SettingSection
        title={t('plugins.setup.settings')}
        description={t('plugins.setup.settingsHint')}
      >
        {!active ? (
          <SettingsEmpty title={t('plugins.setup.offHint')} />
        ) : (
          <PluginSettingsForm
            plugin={plugin}
            projectId={project.id}
            canEdit={canEdit}
          />
        )}
      </SettingSection>
    </>
  );
}
