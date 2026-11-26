import type { PluginSummary, SettingSpec } from '@rustrak/client';
import {
  Button,
  Input,
  Select,
  SelectItem,
  SettingRow,
  SettingsEmpty,
  SettingsListSkeleton,
  Switch,
} from '@rustrak/ui';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { type FormEvent, type ReactNode, useState } from 'react';
import { toast } from 'sonner';
import { useLocale, useTranslations } from 'use-intl';
import { QueryState } from '@/shared/ui/components/query-state';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';
import { pluginMutations } from '../../api/mutations';
import { pluginQueries } from '../../api/queries';
import {
  canReset,
  fieldsAt,
  fieldValue,
  settingsPatch,
  settingText,
} from '../../model/settings';

/**
 * A plugin's settings, as one form for every plugin: drawn from the schema
 * it declares in rustrak.config.ts and labelled with its own messages. For
 * the whole instance, or for one project with `projectId`.
 */
export function PluginSettingsForm({
  plugin,
  projectId,
  canEdit,
}: {
  plugin: PluginSummary;
  projectId?: number;
  canEdit: boolean;
}) {
  const t = useTranslations('settings');
  const locale = useLocale();
  const toastFailure = useToastFailure();
  const level = projectId === undefined ? 'instance' : 'project';
  const fields = fieldsAt(plugin.settings, level);
  const view = useQuery({
    ...pluginQueries.settings(plugin.id, projectId),
    enabled: fields.length > 0,
  });
  const client = useQueryClient();
  const save = useMutation(pluginMutations.saveSettings(plugin.id, projectId));
  const [draft, setDraft] = useState<Record<string, unknown>>({});

  if (fields.length === 0) {
    return <SettingsEmpty title={t('plugins.detail.nothingToConfigure')} />;
  }

  return (
    <QueryState
      query={view}
      placeholder={<SettingsListSkeleton rows={fields.length} />}
      title={t('plugins.form.loadFailed')}
    >
      {(data) => {
        const saved = data.values as Record<string, unknown>;
        const patch = settingsPatch(fields, saved, draft);
        const messages = plugin.messages as Record<string, unknown>;
        const label = (key: string) =>
          settingText(messages, locale, key, 'label');
        const submit = (e: FormEvent) => {
          e.preventDefault();
          save.mutate(patch, {
            onSuccess: (saved) => {
              // The answer is the new state: shown at once, before the refetch.
              client.setQueryData(
                pluginQueries.settings(plugin.id, projectId).queryKey,
                saved,
              );
              setDraft({});
              toast.success(t('plugins.form.saved'));
            },
            onError: (error) =>
              toastFailure(t('plugins.form.saveFailed'), error),
          });
        };

        return (
          <form onSubmit={submit} data-testid="plugin-settings-form">
            {data.missing.length > 0 ? (
              <p
                role="status"
                className="border-b border-border-divider px-5 py-3 text-meta text-warning-fg"
              >
                {t('plugins.form.missing', {
                  fields: data.missing.map(label).join(', '),
                })}
              </p>
            ) : null}
            {fields.map((spec) => (
              <SettingRow
                key={spec.key}
                title={label(spec.key)}
                description={settingText(
                  messages,
                  locale,
                  spec.key,
                  'description',
                )}
                htmlFor={`setting-${spec.key}`}
              >
                <Field
                  spec={spec}
                  value={fieldValue(spec.key, saved, draft)}
                  // What applies when nothing is set here: the instance's value in
                  // a project, the default for the instance.
                  inherited={
                    level === 'project' ? data.resolved[spec.key] : spec.default
                  }
                  secretSet={data.secrets[spec.key] ?? false}
                  disabled={!canEdit || save.isPending}
                  onChange={(value) =>
                    setDraft((d) => ({ ...d, [spec.key]: value }))
                  }
                />
                {canEdit && canReset(spec, saved, draft) ? (
                  <div className="flex justify-end">
                    <Button
                      variant="ghost"
                      size="sm"
                      disabled={save.isPending}
                      onClick={() =>
                        setDraft((d) => ({ ...d, [spec.key]: '' }))
                      }
                    >
                      {t('plugins.form.reset')}
                    </Button>
                  </div>
                ) : null}
              </SettingRow>
            ))}
            {canEdit ? (
              <div className="flex justify-end px-5 py-4">
                <Button
                  type="submit"
                  disabled={Object.keys(patch).length === 0 || save.isPending}
                >
                  {t('plugins.form.save')}
                </Button>
              </div>
            ) : null}
          </form>
        );
      }}
    </QueryState>
  );
}

interface FieldProps {
  spec: SettingSpec;
  value: unknown;
  inherited: unknown;
  secretSet: boolean;
  disabled: boolean;
  onChange: (value: unknown) => void;
}

function Field(props: FieldProps) {
  const Control = CONTROLS[props.spec.type];
  return <Control {...props} id={`setting-${props.spec.key}`} />;
}

type ControlProps = FieldProps & { id: string };

const shown = (v: unknown) =>
  v === undefined || v === null ? undefined : String(v);

function BooleanControl({
  id,
  value,
  inherited,
  disabled,
  onChange,
}: ControlProps) {
  // At the row's end, where every switch in Settings sits.
  return (
    <div className="flex justify-end">
      <Switch
        id={id}
        checked={Boolean(value ?? inherited)}
        disabled={disabled}
        onCheckedChange={onChange}
      />
    </div>
  );
}

function SelectControl({
  id,
  spec,
  value,
  inherited,
  disabled,
  onChange,
}: ControlProps) {
  return (
    <Select
      id={id}
      value={(value ?? inherited ?? null) as string | null}
      onValueChange={onChange}
      disabled={disabled}
    >
      {(spec.options ?? []).map((o) => (
        <SelectItem key={o} value={o}>
          {o}
        </SelectItem>
      ))}
    </Select>
  );
}

function SecretControl({
  id,
  value,
  secretSet,
  disabled,
  onChange,
}: ControlProps) {
  const t = useTranslations('settings');
  return (
    <Input
      id={id}
      type="password"
      autoComplete="off"
      value={(value as string | undefined) ?? ''}
      placeholder={
        secretSet ? t('plugins.form.secretSet') : t('plugins.form.secretNotSet')
      }
      disabled={disabled}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

function NumberControl({
  id,
  spec,
  value,
  inherited,
  disabled,
  onChange,
}: ControlProps) {
  return (
    <Input
      id={id}
      type="number"
      numeric
      min={spec.min ?? undefined}
      max={spec.max ?? undefined}
      step={spec.integer ? 1 : 'any'}
      value={shown(value) ?? ''}
      placeholder={shown(inherited)}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

function TextControl({
  id,
  spec,
  value,
  inherited,
  disabled,
  onChange,
}: ControlProps) {
  return (
    <Input
      id={id}
      type={spec.type === 'url' ? 'url' : 'text'}
      value={(value as string | undefined) ?? ''}
      placeholder={shown(inherited)}
      disabled={disabled}
      onChange={(e) => onChange(e.target.value)}
    />
  );
}

const CONTROLS: Record<
  SettingSpec['type'],
  (props: ControlProps) => ReactNode
> = {
  boolean: BooleanControl,
  select: SelectControl,
  secret: SecretControl,
  number: NumberControl,
  text: TextControl,
  url: TextControl,
};
