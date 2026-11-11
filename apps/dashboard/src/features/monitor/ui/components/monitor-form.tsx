import type { MonitorResponse } from '@rustrak/client';
import { Button, Input, Skeleton } from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useMemo } from 'react';
import { toast } from 'sonner';
import { useTimeZone, useTranslations } from 'use-intl';
import { monitorMutations } from '@/features/monitor/api/mutations';
import { monitorHref, monitorsHref } from '@/features/monitor/lib/href';
import {
  DEFAULT_MONITOR_FORM,
  formFromMonitor,
  inputFromForm,
  type MonitorFormData,
  monitorFormSchema,
} from '@/features/monitor/model/monitor-form';
import { ScheduleField } from '@/features/monitor/ui/components/schedule-field';
import { SchedulePreview } from '@/features/monitor/ui/components/schedule-preview';
import type { ApiError } from '@/shared/api/query';
import { applyServerFieldErrors } from '@/shared/lib/form-errors';
import {
  fieldErrorText,
  revalidateLogic,
  useAppForm,
  useFieldContext,
} from '@/shared/ui/components/app-form';
import { Link } from '@/shared/ui/components/link';
import { TimeZoneCombobox } from '@/shared/ui/components/time-zone-combobox';
import { useRouter } from '@/shared/ui/hooks/use-router';

/**
 * Creates a monitor, or edits one. A full page rather than a dialog: the
 * preview of the next runs is what makes a crontab trustworthy, and it needs
 * the room beside the fields.
 */
export function MonitorForm({
  projectId,
  monitor,
}: {
  projectId: number;
  /** Absent when creating. */
  monitor?: MonitorResponse;
}) {
  const t = useTranslations('crons');
  const globalT = useTranslations();
  const router = useRouter();
  const readerZone = useTimeZone();
  const create = useMutation(monitorMutations.create(projectId));
  const update = useMutation(monitorMutations.update(projectId));
  const isPending = create.isPending || update.isPending;

  const schema = useMemo(() => monitorFormSchema(t), [t]);
  const form = useAppForm({
    defaultValues: monitor
      ? formFromMonitor(monitor)
      : { ...DEFAULT_MONITOR_FORM, timezone: readerZone ?? 'UTC' },
    validationLogic: revalidateLogic(),
    validators: { onDynamic: schema },
    onSubmit: ({ value }) => onSubmit(value),
  });

  const listHref = monitorsHref(projectId);
  const cancelHref = monitor ? monitorHref(projectId, monitor.slug) : listHref;

  function onSubmit(data: MonitorFormData) {
    const input = inputFromForm(data);
    const callbacks = {
      onError: (error: ApiError) => {
        const applied = applyServerFieldErrors(form, error.error, {
          // The server blames `config` as a whole; the schedule is the part
          // of it a person most often gets wrong.
          map: { config: 'schedule' },
          labels: {
            name: t('form.name'),
            slug: t('form.slug'),
            schedule: t('form.sectionSchedule'),
          },
          t: globalT,
        });
        if (applied.formLevel) {
          toast.error(t('form.saveFailed'), { description: applied.formLevel });
        }
      },
      onSuccess: (saved: MonitorResponse) => {
        toast.success(t(monitor ? 'form.saved' : 'form.created'));
        router.push(monitorHref(projectId, saved.slug));
      },
    };
    if (monitor) {
      update.mutate(
        {
          slug: monitor.slug,
          body: { name: input.name, config: input.config },
        },
        callbacks,
      );
    } else {
      create.mutate(input, callbacks);
    }
  }

  return (
    <form.AppForm>
      <form.Form className="grid gap-8 lg:grid-cols-[minmax(0,1fr)_20rem]">
        <div className="space-y-10">
          <Section title={t('form.sectionMonitor')}>
            <div className="grid items-start gap-5 sm:grid-cols-2">
              <form.AppField name="name">
                {(field) => (
                  <field.TextField
                    label={t('form.name')}
                    placeholder={t('form.namePlaceholder')}
                    autoComplete="off"
                    disabled={isPending}
                  />
                )}
              </form.AppField>
              <form.AppField name="slug">
                {(field) => (
                  <field.Item
                    label={t('form.slug')}
                    hint={monitor ? t('form.slugLocked') : t('form.slugHint')}
                  >
                    <BoundInput
                      placeholder={t('form.slugPlaceholder')}
                      autoComplete="off"
                      disabled={isPending || monitor !== undefined}
                      className="font-mono"
                    />
                  </field.Item>
                )}
              </form.AppField>
            </div>
          </Section>

          <Section title={t('form.sectionSchedule')}>
            <div className="space-y-5">
              <form.Field name="schedule">
                {(field) => (
                  <form.Subscribe
                    selector={(state) =>
                      fieldErrorText([
                        ...(state.fieldMeta.schedule?.errors ?? []),
                        // A nested issue (`schedule.value`) is where zod puts it.
                        ...(state.fieldMeta['schedule.value' as 'schedule']
                          ?.errors ?? []),
                      ])
                    }
                  >
                    {(error) => (
                      <ScheduleField
                        value={field.state.value}
                        onChange={field.handleChange}
                        disabled={isPending}
                        error={error}
                      />
                    )}
                  </form.Subscribe>
                )}
              </form.Field>
              <form.AppField name="timezone">
                {(field) => (
                  <field.Item className="max-w-xs" label={t('form.timezone')}>
                    <TimeZoneCombobox
                      value={field.state.value}
                      onChange={(zone) => zone && field.handleChange(zone)}
                      placeholder={t('form.timezonePlaceholder')}
                      noResults={t('form.timezoneNoResults')}
                      disabled={isPending}
                      invalid={
                        fieldErrorText(field.state.meta.errors) !== undefined
                      }
                    />
                  </field.Item>
                )}
              </form.AppField>
            </div>
          </Section>

          <Section title={t('form.sectionTolerance')}>
            <div className="grid items-start gap-5 sm:grid-cols-2">
              {(
                [
                  ['checkin_margin', 'margin', 'minutesSuffix'],
                  ['max_runtime', 'maxRuntime', 'minutesSuffix'],
                  [
                    'failure_issue_threshold',
                    'failureThreshold',
                    'checkInsSuffix',
                  ],
                  ['recovery_threshold', 'recoveryThreshold', 'checkInsSuffix'],
                ] as const
              ).map(([name, label, suffix]) => (
                <form.AppField key={name} name={name}>
                  {(field) => (
                    <field.Item
                      label={t(`form.${label}`)}
                      hint={t(`form.${label}Hint`)}
                    >
                      <div className="flex items-center gap-2">
                        <NumberInput disabled={isPending} />
                        <span className="text-meta text-fg-subtle">
                          {t(`form.${suffix}`)}
                        </span>
                      </div>
                    </field.Item>
                  )}
                </form.AppField>
              ))}
            </div>
          </Section>

          <div className="flex items-center justify-end gap-2 border-border-subtle border-t pt-6">
            <Button variant="secondary" render={<Link href={cancelHref} />}>
              {t('form.cancel')}
            </Button>
            <Button variant="primary" type="submit" loading={isPending}>
              {t(monitor ? 'form.save' : 'form.create')}
            </Button>
          </div>
          <form.RootError />
        </div>

        <aside className="self-start lg:sticky lg:top-6">
          <form.Subscribe
            selector={(state) =>
              [
                state.values.schedule,
                state.values.timezone,
                state.values.checkin_margin,
              ] as const
            }
          >
            {([schedule, timezone, margin]) => (
              <Preview
                schedule={schedule}
                timezone={timezone}
                margin={margin}
              />
            )}
          </form.Subscribe>
        </aside>
      </form.Form>
    </form.AppForm>
  );
}

/**
 * The form's layout with its fields still to come: what the edit page shows
 * while the monitor loads.
 */
export function MonitorFormSkeleton() {
  const t = useTranslations('crons');
  const field = (key: number) => (
    <div key={key} className="flex flex-col gap-2">
      <Skeleton className="h-3.5 w-24" />
      <Skeleton className="h-control-lg w-full" />
    </div>
  );
  return (
    <div className="grid gap-8 lg:grid-cols-[minmax(0,1fr)_20rem]">
      <div className="space-y-10">
        <Section title={t('form.sectionMonitor')}>
          <div className="grid items-start gap-5 sm:grid-cols-2">
            {[0, 1].map(field)}
          </div>
        </Section>
        <Section title={t('form.sectionSchedule')}>
          <div className="grid items-start gap-5 sm:grid-cols-2">
            {[0, 1].map(field)}
          </div>
        </Section>
        <Section title={t('form.sectionTolerance')}>
          <div className="grid items-start gap-5 sm:grid-cols-2">
            {[0, 1, 2, 3].map(field)}
          </div>
        </Section>
      </div>
      <Skeleton className="h-48 w-full self-start" />
    </div>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="space-y-4">
      <h2 className="text-card-title text-fg">{title}</h2>
      {children}
    </section>
  );
}

function BoundInput(
  props: Omit<
    React.ComponentProps<typeof Input>,
    'value' | 'onChange' | 'onBlur' | 'name' | 'invalid'
  >,
) {
  const field = useFieldContext<string>();
  return (
    <Input
      {...props}
      name={field.name}
      value={field.state.value}
      onChange={(event) => field.handleChange(event.target.value)}
      onBlur={field.handleBlur}
      invalid={fieldErrorText(field.state.meta.errors) !== undefined}
    />
  );
}

function NumberInput({ disabled }: { disabled: boolean }) {
  const field = useFieldContext<number>();
  return (
    <Input
      type="number"
      inputMode="numeric"
      min={1}
      disabled={disabled}
      boxClassName="w-28"
      className="tabular-nums"
      name={field.name}
      onBlur={field.handleBlur}
      value={Number.isNaN(field.state.value) ? '' : field.state.value}
      onChange={(event) => field.handleChange(event.target.valueAsNumber)}
      invalid={fieldErrorText(field.state.meta.errors) !== undefined}
    />
  );
}

/** The next runs of the schedule being typed, as the server expects them. */
function Preview({
  schedule,
  timezone,
  margin,
}: Pick<MonitorFormData, 'schedule' | 'timezone'> & { margin: number }) {
  const t = useTranslations('crons');
  const filled =
    schedule.type === 'crontab'
      ? schedule.value.trim() !== ''
      : schedule.value >= 1;

  return (
    <SchedulePreview
      schedule={filled ? schedule : null}
      timezone={timezone}
      note={t('form.previewNote', {
        timezone,
        margin: Number.isNaN(margin) ? 1 : margin,
      })}
    />
  );
}
