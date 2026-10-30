import { zodResolver } from '@hookform/resolvers/zod';
import type {
  AlertIntegration,
  AlertRuleResponse,
  AlertType,
} from '@rustrak/client';
import { Button, cn, Input, Skeleton, TestIcon, Text } from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { type ReactNode, useMemo, useState, useTransition } from 'react';
import { FormProvider, useForm, useFormContext } from 'react-hook-form';
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
  cooldownUnit,
  DEFAULT_COOLDOWN,
} from '@/features/alert/model/rule-summary';
import { useChannelRouting } from '@/features/alert/ui/hooks/use-channel-routing';
import type { ApiError } from '@/shared/api/query';
import { applyServerFieldErrors } from '@/shared/lib/form-errors';
import {
  conditionsFromForm,
  environmentsOf,
  formFromConditions,
  levels,
} from '@/shared/lib/rule-conditions';
import { Link } from '@/shared/ui/components/link';
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
  const create = useMutation(alertMutations.createRule(projectId));
  const update = useMutation(alertMutations.updateRule(projectId));
  const test = useMutation(alertMutations.testIntegration());
  const isPending = create.isPending || update.isPending;
  const [isTesting, startTest] = useTransition();
  const routing = useChannelRouting(rule, integrations);
  const schema = useMemo(() => alertRuleFormSchema(t), [t]);
  const form = useForm<AlertRuleFormData>({
    resolver: zodResolver(schema),
    defaultValues: {
      name: rule?.name ?? '',
      alert_type: rule?.alert_type ?? initialType,
      selected_integration_ids: rule?.integration_ids ?? [],
      is_enabled: rule?.is_enabled ?? true,
      cooldown_minutes: rule?.cooldown_minutes ?? DEFAULT_COOLDOWN,
      ...formFromConditions(rule?.conditions ?? {}),
      environments: environmentsOf(rule?.conditions ?? {}),
    },
  });

  const values = form.watch();
  const selected = new Set(values.selected_integration_ids);
  const chosen = integrations.filter((i) => selected.has(i.id));

  const onSubmit = (data: AlertRuleFormData) => {
    if (!routing.validate(data.selected_integration_ids)) return;
    if (data.selected_integration_ids.length === 0) {
      form.setError('selected_integration_ids', {
        message: t('ruleDialog.selectAtLeastOne'),
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
  };

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
                toast.error(t('editor.testFailed', { channel }), {
                  description: error.error.message,
                }),
            );
        }),
      );
    });

  return (
    <FormProvider {...form}>
      <form
        onSubmit={form.handleSubmit(onSubmit)}
        className="flex min-h-0 flex-1 max-lg:flex-col max-lg:overflow-y-auto"
      >
        <div className="flex min-w-0 flex-1 flex-col divide-y divide-border-divider lg:overflow-y-auto">
          <Row label={t('editor.name')}>
            <Input
              placeholder={t('editor.namePlaceholder')}
              invalid={Boolean(form.formState.errors.name)}
              {...form.register('name')}
            />
            {form.formState.errors.name ? (
              <span className="text-sev-error-fg text-meta">
                {form.formState.errors.name.message}
              </span>
            ) : null}
          </Row>

          <TriggerRow />
          <EnvironmentsRow environments={environments} />
          <LevelRow />

          {values.alert_type === 'new_issue' ? (
            <Row label={t('editor.minEvents')} hint={t('editor.minEventsHint')}>
              <Input
                type="number"
                min={1}
                boxClassName="w-28"
                invalid={Boolean(form.formState.errors.min_events)}
                {...form.register('min_events', { valueAsNumber: true })}
              />
              <FieldError name="min_events" />
            </Row>
          ) : null}

          <Row
            label={t('editor.notify')}
            hint={t.rich('editor.notifyHint', {
              link: (chunks) => (
                <Link
                  href={`/settings/integrations?projectId=${projectId}`}
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
              <ChannelPicker
                integrations={integrations}
                routing={routing}
                disabled={isPending}
              />
            )}
          </Row>

          <FrequencyRow />
        </div>

        {/* The rail: the sentence the form adds up to, and what to do with it. */}
        <aside className="flex flex-col gap-4 border-border-divider bg-panel px-5 py-5 max-lg:border-t lg:w-88 lg:shrink-0 lg:border-l xl:w-96">
          <h2 className="font-mono text-column text-fg-meta uppercase">
            {t('editor.summary')}
          </h2>
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
      </form>
    </FormProvider>
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
      <aside className="flex flex-col gap-4 border-border-divider bg-panel px-5 py-5 max-lg:border-t lg:w-88 lg:shrink-0 lg:border-l xl:w-96">
        <h2 className="font-mono text-column text-fg-meta uppercase">
          {t('editor.summary')}
        </h2>
        <div className="flex flex-col gap-2">
          <Skeleton className="h-4 w-full" />
          <Skeleton className="h-4 w-2/3" />
        </div>
      </aside>
    </div>
  );
}

const dirty = { shouldDirty: true } as const;

/** A field's validation message, under it. */
function FieldError({ name }: { name: 'cooldown_minutes' | 'min_events' }) {
  const { formState } = useFormContext<AlertRuleFormData>();
  const message = formState.errors[name]?.message;
  return message ? (
    <span className="text-sev-error-fg text-meta">{message}</span>
  ) : null;
}

/** What the alert fires on, one card per trigger. */
function TriggerRow() {
  const t = useTranslations('alerts');
  const form = useFormContext<AlertRuleFormData>();
  const current = form.watch('alert_type');
  return (
    <Row label={t('editor.when')}>
      <div className="grid gap-2 sm:grid-cols-2">
        {alertTypes.map(({ type }) => (
          <button
            key={type}
            type="button"
            aria-pressed={current === type}
            onClick={() => {
              form.setValue('alert_type', type, dirty);
              // Only new issues show the event count; one left invalid
              // there would block the save from a field no longer on screen.
              const minEvents = form.getValues('min_events');
              if (
                type !== 'new_issue' &&
                !(Number.isInteger(minEvents) && minEvents >= 1)
              ) {
                form.setValue('min_events', 1);
                form.clearErrors('min_events');
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
function EnvironmentsRow({ environments }: { environments: string[] }) {
  const t = useTranslations('alerts');
  const form = useFormContext<AlertRuleFormData>();
  const [draft, setDraft] = useState('');
  const chosenEnvs = form.watch('environments');
  const chosen = new Set(chosenEnvs);
  const envOptions = [...new Set([...environments, ...chosenEnvs])];
  const setEnvs = (next: string[]) =>
    form.setValue('environments', next, dirty);
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
function LevelRow() {
  const t = useTranslations('alerts');
  const form = useFormContext<AlertRuleFormData>();
  const [type, minLevel] = form.watch(['alert_type', 'min_level']);
  if (type === 'new_feedback') return null;
  return (
    <Row label={t('editor.level')}>
      <div className="flex flex-wrap gap-1.5">
        {(['any', ...levels] as const).map((level) => (
          <Chip
            key={level}
            selected={minLevel === level}
            onClick={() => form.setValue('min_level', level, dirty)}
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
function FrequencyRow() {
  const t = useTranslations('alerts');
  const form = useFormContext<AlertRuleFormData>();
  const cooldown = form.watch('cooldown_minutes');
  return (
    <Row label={t('editor.frequency')} hint={t('editor.frequencyHint')}>
      <div className="flex flex-wrap items-center gap-1.5">
        {COOLDOWN_PRESETS.map((minutes) => {
          const unit = cooldownUnit(minutes);
          return (
            <Chip
              key={minutes}
              selected={cooldown === minutes}
              onClick={() => form.setValue('cooldown_minutes', minutes, dirty)}
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
          invalid={Boolean(form.formState.errors.cooldown_minutes)}
          {...form.register('cooldown_minutes', { valueAsNumber: true })}
        />
      </div>
      <FieldError name="cooldown_minutes" />
    </Row>
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
