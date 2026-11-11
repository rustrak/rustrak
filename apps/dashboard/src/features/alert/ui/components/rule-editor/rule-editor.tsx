import type {
  AlertIntegration,
  AlertRuleResponse,
  AlertType,
} from '@rustrak/client';
import { Button, cn, Input, Skeleton, TestIcon, Text } from '@rustrak/ui';
import { useSelector } from '@tanstack/react-form';
import { useMutation } from '@tanstack/react-query';
import { type ReactNode, useMemo, useState, useTransition } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { alertMutations } from '@/features/alert/api/mutations';
import { buildChannelsPayload } from '@/features/alert/lib/routing';
import {
  type AlertRuleFormData,
  alertRuleFormSchema,
} from '@/features/alert/model/alert-rule-form';
import { alertTypes } from '@/features/alert/model/alert-types';
import {
  COOLDOWN_PRESETS,
  channelsHref,
  cooldownUnit,
  DEFAULT_COOLDOWN,
} from '@/features/alert/model/rule-summary';
import { useChannelRouting } from '@/features/alert/ui/hooks/use-channel-routing';
import type { ApiError } from '@/shared/api/query';
import {
  applyServerFieldErrors,
  setServerErrors,
} from '@/shared/lib/form-errors';
import {
  conditionsFromForm,
  environmentsOf,
  formFromConditions,
  levels,
} from '@/shared/lib/rule-conditions';
import {
  fieldErrorText,
  revalidateLogic,
  useAppForm,
} from '@/shared/ui/components/app-form';
import { Link } from '@/shared/ui/components/link';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';
import { RuleSentence } from '../rule-sentence';
import { ChannelPicker } from './channel-picker/channel-picker';

/**
 * One alert's form, the whole screen: a line per question -- what it is
 * called, what it fires on, which environments and levels count, whom it
 * tells, how often at most -- and in the rail the sentence it adds up to,
 * beside the save button, so what is saved is never a surprise.
 */
export function RuleEditor({
  projectId,
  rule,
  initialType,
  integrations,
  environments,
  onSaved,
  onCancel,
}: {
  projectId: number;
  rule: AlertRuleResponse | null;
  /** The trigger a new alert starts on. */
  initialType: AlertType;
  integrations: AlertIntegration[];
  /** The project's environments, offered as choices. */
  environments: string[];
  onSaved: (rule: AlertRuleResponse) => void;
  onCancel: () => void;
}) {
  const t = useTranslations('alerts');
  const globalT = useTranslations();
  const toastFailure = useToastFailure();
  const create = useMutation(alertMutations.createRule(projectId));
  const update = useMutation(alertMutations.updateRule(projectId));
  const test = useMutation(alertMutations.testIntegration());
  const isPending = create.isPending || update.isPending;
  const [isTesting, startTest] = useTransition();
  const routing = useChannelRouting(rule, integrations);
  const schema = useMemo(() => alertRuleFormSchema(t), [t]);
  const form = useRuleForm(
    {
      name: rule?.name ?? '',
      alert_type: rule?.alert_type ?? initialType,
      selected_integration_ids: rule?.integration_ids ?? [],
      is_enabled: rule?.is_enabled ?? true,
      cooldown_minutes: rule?.cooldown_minutes ?? DEFAULT_COOLDOWN,
      ...formFromConditions(rule?.conditions ?? {}),
      environments: environmentsOf(rule?.conditions ?? {}),
    },
    schema,
    (data) => onSubmit(data),
  );

  const values = useSelector(form.store, (state) => state.values);
  const selected = new Set(values.selected_integration_ids);
  const chosen = integrations.filter((i) => selected.has(i.id));

  function onSubmit(data: AlertRuleFormData) {
    if (!routing.validate(data.selected_integration_ids)) return;
    if (data.selected_integration_ids.length === 0) {
      setServerErrors(form, {
        fields: { selected_integration_ids: t('ruleDialog.selectAtLeastOne') },
      });
      return;
    }
    const body = {
      name: data.name,
      alert_type: data.alert_type,
      channels: buildChannelsPayload(
        data.selected_integration_ids,
        routing.routingMap,
        integrations,
      ),
      cooldown_minutes: data.cooldown_minutes,
      conditions: conditionsFromForm(data),
    };
    const callbacks = {
      onError: (error: ApiError) => {
        const applied = applyServerFieldErrors(form, error.error, {
          labels: { name: t('common.fieldName') },
          t: globalT,
        });
        if (applied.formLevel) {
          toast.error(t('ruleDialog.saveFailed'), {
            description: applied.formLevel,
          });
        }
      },
      onSuccess: (saved: AlertRuleResponse) => {
        toast.success(t(rule ? 'ruleDialog.updated' : 'ruleDialog.created'));
        onSaved(saved);
      },
    };
    if (rule) update.mutate({ ruleId: rule.id, body }, callbacks);
    else create.mutate(body, callbacks);
  }

  // A test sends through every chosen channel, routed as the saved rule
  // would be: the same payload, so an email gets its recipients as a list.
  // Several calls on one mutation: `mutateAsync` reports each, where
  // `mutate`'s callbacks would only answer the last, and the transition
  // spans them all where `isPending` would only follow the last.
  const sendTest = () =>
    startTest(async () => {
      if (!routing.validate(values.selected_integration_ids)) return;
      const payload = buildChannelsPayload(
        values.selected_integration_ids,
        routing.routingMap,
        integrations,
      );
      const names = new Map(integrations.map((i) => [i.id, i.name]));
      await Promise.all(
        payload.map(({ integration_id, routing_override }) => {
          const channel = names.get(integration_id) ?? String(integration_id);
          return test
            .mutateAsync({
              id: integration_id,
              routingOverride: routing_override,
            })
            .then(
              (answer) => {
                if (answer.success) {
                  toast.success(t('editor.testSent', { channel }));
                } else {
                  toast.error(t('editor.testFailed', { channel }), {
                    description: answer.message,
                  });
                }
              },
              (error: ApiError) =>
                toastFailure(t('editor.testFailed', { channel }), error),
            );
        }),
      );
    });

  return (
    <form.AppForm>
      <form.Form className="flex min-h-0 flex-1 max-lg:flex-col max-lg:overflow-y-auto">
        <div className="flex min-w-0 flex-1 flex-col divide-y divide-border-divider lg:overflow-y-auto">
          <form.Field name="name">
            {(field) => {
              const error = fieldErrorText(field.state.meta.errors);
              return (
                <Row label={t('editor.name')}>
                  <Input
                    name={field.name}
                    placeholder={t('editor.namePlaceholder')}
                    invalid={Boolean(error)}
                    value={field.state.value}
                    onChange={(e) => field.handleChange(e.target.value)}
                    onBlur={field.handleBlur}
                  />
                  <FieldMessage error={error} />
                </Row>
              );
            }}
          </form.Field>

          <TriggerRow form={form} />
          <EnvironmentsRow form={form} environments={environments} />
          <LevelRow form={form} />

          {values.alert_type === 'new_issue' ? (
            <form.Field name="min_events">
              {(field) => {
                const error = fieldErrorText(field.state.meta.errors);
                return (
                  <Row
                    label={t('editor.minEvents')}
                    hint={t('editor.minEventsHint')}
                  >
                    <Input
                      type="number"
                      min={1}
                      boxClassName="w-28"
                      name={field.name}
                      invalid={Boolean(error)}
                      value={numberValue(field.state.value)}
                      onChange={(e) =>
                        field.handleChange(e.target.valueAsNumber)
                      }
                      onBlur={field.handleBlur}
                    />
                    <FieldMessage error={error} />
                  </Row>
                );
              }}
            </form.Field>
          ) : null}

          <Row
            label={t('editor.notify')}
            hint={t.rich('editor.notifyHint', {
              link: (chunks) => (
                <Link
                  href={channelsHref(projectId)}
                  className="text-fg-secondary underline hover:text-fg"
                >
                  {chunks}
                </Link>
              ),
            })}
          >
            {integrations.length === 0 ? (
              <Text variant="meta" tone="subtle">
                {t('editor.noChannels')}
              </Text>
            ) : (
              <form.Field name="selected_integration_ids">
                {(field) => (
                  <ChannelPicker
                    integrations={integrations}
                    routing={routing}
                    disabled={isPending}
                    value={field.state.value}
                    onChange={field.handleChange}
                    error={fieldErrorText(field.state.meta.errors)}
                  />
                )}
              </form.Field>
            )}
          </Row>

          <FrequencyRow form={form} />
        </div>

        {/* The rail: the sentence the form adds up to, and what to do with it. */}
        <aside className="flex flex-col gap-4 border-border-divider bg-aside px-5 py-5 max-lg:border-t lg:w-88 lg:shrink-0 lg:border-l xl:w-96">
          <h2 className="text-column text-fg-meta">{t('editor.summary')}</h2>
          <p className="text-body text-fg-secondary leading-relaxed">
            <RuleSentence
              rule={{
                type: values.alert_type,
                environments: values.environments,
                // Feedback has no level; one left in the form is not saved.
                minLevel:
                  values.alert_type === 'new_feedback'
                    ? 'any'
                    : values.min_level,
                minEvents: values.min_events,
                cooldownMinutes: values.cooldown_minutes,
                // Dispatch skips a disabled integration; so does the sentence.
                channels: chosen.filter((i) => i.is_enabled).map((i) => i.name),
              }}
            />
          </p>
          <div className="flex flex-col gap-2 border-border-divider border-t pt-4">
            <Button type="submit" variant="primary" loading={isPending}>
              {t(rule ? 'ruleDialog.saveChanges' : 'editor.create')}
            </Button>
            <Button
              icon={TestIcon}
              loading={isTesting}
              disabled={values.selected_integration_ids.length === 0}
              onClick={sendTest}
            >
              {t('editor.test')}
            </Button>
            <Button variant="ghost" onClick={onCancel}>
              {t('common.cancel')}
            </Button>
          </div>
        </aside>
      </form.Form>
    </form.AppForm>
  );
}

/**
 * The editor's layout with its fields still to come: what a page holding
 * the editor shows while the rule and its channels load.
 */
export function RuleEditorSkeleton() {
  const t = useTranslations('alerts');
  const rows = [
    [t('editor.name'), 'h-9 w-full'],
    [t('editor.when'), 'h-16 w-full'],
    [t('editor.environments'), 'h-7 w-64'],
    [t('editor.level'), 'h-7 w-72'],
    [t('editor.notify'), 'h-9 w-full'],
    [t('editor.frequency'), 'h-7 w-80'],
  ] as const;
  return (
    <div className="flex min-h-0 flex-1 max-lg:flex-col max-lg:overflow-y-auto">
      <div className="flex min-w-0 flex-1 flex-col divide-y divide-border-divider lg:overflow-y-auto">
        {rows.map(([label, size]) => (
          <Row key={label} label={label}>
            <Skeleton className={cn('max-w-full', size)} />
          </Row>
        ))}
      </div>
      <aside className="flex flex-col gap-4 border-border-divider bg-aside px-5 py-5 max-lg:border-t lg:w-88 lg:shrink-0 lg:border-l xl:w-96">
        <h2 className="text-column text-fg-meta">{t('editor.summary')}</h2>
        <div className="flex flex-col gap-2">
          <Skeleton className="h-4 w-full" />
          <Skeleton className="h-4 w-2/3" />
        </div>
      </aside>
    </div>
  );
}

function useRuleForm(
  defaultValues: AlertRuleFormData,
  schema: ReturnType<typeof alertRuleFormSchema>,
  onSubmit: (data: AlertRuleFormData) => void,
) {
  return useAppForm({
    defaultValues,
    validationLogic: revalidateLogic(),
    validators: { onDynamic: schema },
    onSubmit: ({ value }) => onSubmit(value),
  });
}

type RuleForm = ReturnType<typeof useRuleForm>;

/** An emptied number input holds NaN; it shows as empty, not as "NaN". */
const numberValue = (value: number) => (Number.isNaN(value) ? '' : value);

/** A field's validation message, under it. */
function FieldMessage({ error }: { error: string | undefined }) {
  return error ? (
    <span className="text-sev-error-fg text-meta">{error}</span>
  ) : null;
}

/** What the alert fires on, one card per trigger. */
function TriggerRow({ form }: { form: RuleForm }) {
  const t = useTranslations('alerts');
  const current = useSelector(form.store, (state) => state.values.alert_type);
  return (
    <Row label={t('editor.when')}>
      <div className="grid gap-2 sm:grid-cols-2">
        {alertTypes.map(({ type }) => (
          <button
            key={type}
            type="button"
            aria-pressed={current === type}
            onClick={() => {
              form.setFieldValue('alert_type', type);
              // Only new issues show the event count; one left invalid
              // there would block the save from a field no longer on screen.
              const minEvents = form.getFieldValue('min_events');
              if (
                type !== 'new_issue' &&
                !(Number.isInteger(minEvents) && minEvents >= 1)
              ) {
                form.setFieldValue('min_events', 1);
              }
            }}
            className={cn(
              'flex flex-col gap-0.5 rounded-lg border px-3 py-2 text-left transition-colors hover:bg-surface-hover',
              current === type
                ? 'border-border-brand bg-success-surface'
                : 'border-border',
            )}
          >
            <span className="font-medium text-body text-fg">
              {t(`types.${type}.name`)}
            </span>
            <span className="text-fg-subtle text-hint">
              {t(`types.${type}.description`)}
            </span>
          </button>
        ))}
      </div>
    </Row>
  );
}

/**
 * Which environments count: all, or the ones picked -- from those the project
 * has seen, or a name typed in, so a rule can wait for an environment that has
 * not sent anything yet.
 */
function EnvironmentsRow({
  form,
  environments,
}: {
  form: RuleForm;
  environments: string[];
}) {
  const t = useTranslations('alerts');
  const [draft, setDraft] = useState('');
  const chosenEnvs = useSelector(
    form.store,
    (state) => state.values.environments,
  );
  const chosen = new Set(chosenEnvs);
  const envOptions = [...new Set([...environments, ...chosenEnvs])];
  const setEnvs = (next: string[]) => form.setFieldValue('environments', next);
  const add = () => {
    const name = draft.trim();
    if (name && !chosen.has(name)) setEnvs([...chosenEnvs, name]);
    setDraft('');
  };
  return (
    <Row label={t('editor.environments')} hint={t('editor.environmentsHint')}>
      <div className="flex flex-wrap items-center gap-1.5">
        <Chip selected={chosen.size === 0} onClick={() => setEnvs([])}>
          {t('editor.allEnvironments')}
        </Chip>
        {envOptions.map((env) => (
          <Chip
            key={env}
            mono
            selected={chosen.has(env)}
            onClick={() =>
              setEnvs(
                chosen.has(env)
                  ? chosenEnvs.filter((e) => e !== env)
                  : [...chosenEnvs, env],
              )
            }
          >
            {env}
          </Chip>
        ))}
        <Input
          value={draft}
          placeholder={t('editor.newEnvironment')}
          aria-label={t('editor.newEnvironment')}
          boxClassName="w-40"
          className="font-mono"
          onChange={(e) => setDraft(e.target.value)}
          // Leaving the field adds what it holds: clicking Save blurs it
          // first, so a typed name is never dropped from the rule.
          onBlur={add}
          onKeyDown={(e) => {
            if (e.key !== 'Enter') return;
            e.preventDefault();
            add();
          }}
          action={
            <Button size="sm" variant="ghost" onClick={add}>
              {t('editor.addEnvironment')}
            </Button>
          }
        />
      </div>
    </Row>
  );
}

/** The lowest level that counts; feedback has none. */
function LevelRow({ form }: { form: RuleForm }) {
  const t = useTranslations('alerts');
  const type = useSelector(form.store, (state) => state.values.alert_type);
  const minLevel = useSelector(form.store, (state) => state.values.min_level);
  if (type === 'new_feedback') return null;
  return (
    <Row label={t('editor.level')}>
      <div className="flex flex-wrap gap-1.5">
        {(['any', ...levels] as const).map((level) => (
          <Chip
            key={level}
            selected={minLevel === level}
            onClick={() => form.setFieldValue('min_level', level)}
          >
            {level === 'any'
              ? t('editor.anyLevel')
              : t('editor.levelOrHigher', { level })}
          </Chip>
        ))}
      </div>
    </Row>
  );
}

/** How often the alert may notify: a preset, or minutes typed in. */
function FrequencyRow({ form }: { form: RuleForm }) {
  const t = useTranslations('alerts');
  return (
    <form.Field name="cooldown_minutes">
      {(field) => {
        const error = fieldErrorText(field.state.meta.errors);
        return (
          <Row label={t('editor.frequency')} hint={t('editor.frequencyHint')}>
            <div className="flex flex-wrap items-center gap-1.5">
              {COOLDOWN_PRESETS.map((minutes) => {
                const unit = cooldownUnit(minutes);
                return (
                  <Chip
                    key={minutes}
                    selected={field.state.value === minutes}
                    onClick={() => field.handleChange(minutes)}
                  >
                    {unit
                      ? t(`editor.every.${unit.unit}`, { count: unit.value })
                      : t('editor.everyTime')}
                  </Chip>
                );
              })}
              <Input
                type="number"
                min={0}
                aria-label={t('editor.customMinutes')}
                boxClassName="w-28"
                action={
                  <span className="pe-2 text-fg-subtle text-meta">
                    {t('ruleDialog.minutesUnit')}
                  </span>
                }
                name={field.name}
                invalid={Boolean(error)}
                value={numberValue(field.state.value)}
                onChange={(e) => field.handleChange(e.target.valueAsNumber)}
                onBlur={field.handleBlur}
              />
            </div>
            <FieldMessage error={error} />
          </Row>
        );
      }}
    </form.Field>
  );
}

/** One question of the form: its label on the left, its answer on the right. */
function Row({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: ReactNode;
  children: ReactNode;
}) {
  return (
    <div className="grid gap-2 px-page-gutter py-5 md:grid-cols-[14rem_minmax(0,1fr)] md:gap-8">
      <div className="flex flex-col gap-0.5 pt-1">
        <span className="font-medium text-body text-fg">{label}</span>
        {hint ? <span className="text-fg-subtle text-hint">{hint}</span> : null}
      </div>
      <div className="flex min-w-0 flex-col gap-2">{children}</div>
    </div>
  );
}

/** One option of a choice, pressed or not. */
function Chip({
  selected,
  mono,
  onClick,
  children,
}: {
  selected: boolean;
  mono?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <Button
      size="sm"
      variant={selected ? 'secondary' : 'ghost'}
      selected={selected}
      onClick={onClick}
      className={cn(mono && 'font-mono', !selected && 'border border-border')}
    >
      {children}
    </Button>
  );
}
