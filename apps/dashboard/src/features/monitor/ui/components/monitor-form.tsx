import { zodResolver } from '@hookform/resolvers/zod';
import type { MonitorResponse } from '@rustrak/client';
import { Skeleton } from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { Loader2 } from 'lucide-react';
import { useMemo } from 'react';
import { type Path, useForm, useFormContext, useWatch } from 'react-hook-form';
import { toast } from 'sonner';
import { useTimeZone, useTranslations } from 'use-intl';
import { monitorMutations } from '@/features/monitor/api/mutations';
import { monitorHref } from '@/features/monitor/lib/href';
import {
  DEFAULT_MONITOR_FORM,
  formFromMonitor,
  inputFromForm,
  type MonitorFormData,
  monitorFormSchema,
} from '@/features/monitor/model/monitor-form';
import type { ApiError } from '@/shared/api/query';
import { applyServerFieldErrors } from '@/shared/lib/form-errors';
import { Link } from '@/shared/ui/components/link';
import { ScheduleField } from '@/shared/ui/components/schedule-field';
import { SchedulePreview } from '@/shared/ui/components/schedule-preview';
import { Button } from '@/shared/ui/components/shadcn/button';
import {
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  FormRootError,
} from '@/shared/ui/components/shadcn/form';
import { Input } from '@/shared/ui/components/shadcn/input';
import { TimeZoneCombobox } from '@/shared/ui/components/time-zone-combobox';
import { useRouter } from '@/shared/ui/hooks/use-router';

const LABEL =
  'text-xs font-bold uppercase tracking-widest text-muted-foreground';

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
  const form = useForm<MonitorFormData>({
    resolver: zodResolver(schema),
    defaultValues: monitor
      ? formFromMonitor(monitor)
      : { ...DEFAULT_MONITOR_FORM, timezone: readerZone ?? 'UTC' },
  });

  const listHref = `/projects/${projectId}/crons`;
  const cancelHref = monitor ? monitorHref(projectId, monitor.slug) : listHref;

  const onSubmit = (data: MonitorFormData) => {
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
  };

  return (
    <Form {...form}>
      <form
        onSubmit={form.handleSubmit(onSubmit)}
        className="grid gap-8 lg:grid-cols-[minmax(0,1fr)_20rem]"
      >
        <div className="space-y-10">
          <Section title={t('form.sectionMonitor')}>
            <div className="grid items-start gap-5 sm:grid-cols-2">
              <TextField
                name="name"
                label={t('form.name')}
                placeholder={t('form.namePlaceholder')}
                disabled={isPending}
              />
              <TextField
                name="slug"
                label={t('form.slug')}
                placeholder={t('form.slugPlaceholder')}
                hint={monitor ? t('form.slugLocked') : t('form.slugHint')}
                disabled={isPending || monitor !== undefined}
                mono
              />
            </div>
          </Section>

          <Section title={t('form.sectionSchedule')}>
            <ScheduleFields disabled={isPending} />
          </Section>

          <Section title={t('form.sectionTolerance')}>
            <div className="grid items-start gap-5 sm:grid-cols-2">
              <NumberField
                name="checkin_margin"
                label={t('form.margin')}
                hint={t('form.marginHint')}
                suffix={t('form.minutesSuffix')}
                disabled={isPending}
              />
              <NumberField
                name="max_runtime"
                label={t('form.maxRuntime')}
                hint={t('form.maxRuntimeHint')}
                suffix={t('form.minutesSuffix')}
                disabled={isPending}
              />
              <NumberField
                name="failure_issue_threshold"
                label={t('form.failureThreshold')}
                hint={t('form.failureThresholdHint')}
                suffix={t('form.checkInsSuffix')}
                disabled={isPending}
              />
              <NumberField
                name="recovery_threshold"
                label={t('form.recoveryThreshold')}
                hint={t('form.recoveryThresholdHint')}
                suffix={t('form.checkInsSuffix')}
                disabled={isPending}
              />
            </div>
          </Section>

          <div className="flex items-center justify-end gap-2 border-t pt-6">
            <Button
              variant="outline"
              nativeButton={false}
              render={<Link href={cancelHref} />}
            >
              {t('form.cancel')}
            </Button>
            <Button type="submit" disabled={isPending}>
              {isPending && <Loader2 className="animate-spin" />}
              {t(monitor ? 'form.save' : 'form.create')}
            </Button>
          </div>
          <FormRootError />
        </div>

        <aside className="self-start lg:sticky lg:top-6">
          <Preview />
        </aside>
      </form>
    </Form>
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
      <Skeleton className="h-9 w-full" />
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
      <h2 className="text-sm font-semibold">{title}</h2>
      {children}
    </section>
  );
}

function TextField({
  name,
  label,
  placeholder,
  hint,
  disabled,
  mono,
}: {
  name: 'name' | 'slug';
  label: string;
  placeholder?: string;
  hint?: string;
  disabled: boolean;
  mono?: boolean;
}) {
  const { control } = useFormContext<MonitorFormData>();
  return (
    <FormField
      control={control}
      name={name}
      render={({ field }) => (
        <FormItem>
          <FormLabel className={LABEL}>{label}</FormLabel>
          <FormControl>
            <Input
              placeholder={placeholder}
              autoComplete="off"
              disabled={disabled}
              className={mono ? 'font-mono' : undefined}
              {...field}
            />
          </FormControl>
          {hint && <FormDescription>{hint}</FormDescription>}
          <FormMessage />
        </FormItem>
      )}
    />
  );
}

type NumberName = Extract<
  Path<MonitorFormData>,
  | 'checkin_margin'
  | 'max_runtime'
  | 'failure_issue_threshold'
  | 'recovery_threshold'
>;

function NumberField({
  name,
  label,
  hint,
  suffix,
  disabled,
}: {
  name: NumberName;
  label: string;
  hint?: string;
  suffix?: string;
  disabled: boolean;
}) {
  const { control } = useFormContext<MonitorFormData>();
  return (
    <FormField
      control={control}
      name={name}
      render={({ field }) => (
        <FormItem>
          <FormLabel className={LABEL}>{label}</FormLabel>
          <div className="flex items-center gap-2">
            <FormControl>
              <Input
                type="number"
                inputMode="numeric"
                min={1}
                disabled={disabled}
                className="w-28 tabular-nums"
                name={field.name}
                ref={field.ref}
                onBlur={field.onBlur}
                value={Number.isNaN(field.value) ? '' : field.value}
                onChange={(event) => field.onChange(event.target.valueAsNumber)}
              />
            </FormControl>
            {suffix && (
              <span className="text-sm text-muted-foreground">{suffix}</span>
            )}
          </div>
          {hint && <FormDescription>{hint}</FormDescription>}
          <FormMessage />
        </FormItem>
      )}
    />
  );
}

function ScheduleFields({ disabled }: { disabled: boolean }) {
  const t = useTranslations('crons');
  const { control } = useFormContext<MonitorFormData>();

  return (
    <div className="space-y-5">
      <FormField
        control={control}
        name="schedule"
        render={({ field, fieldState }) => (
          <ScheduleField
            value={field.value}
            onChange={field.onChange}
            disabled={disabled}
            error={
              fieldState.error?.message ??
              // A nested issue (`schedule.value`) is where zod puts it.
              (fieldState.error as { value?: { message?: string } } | undefined)
                ?.value?.message
            }
          />
        )}
      />
      <FormField
        control={control}
        name="timezone"
        render={({ field }) => (
          <FormItem className="max-w-xs">
            <FormLabel className={LABEL}>{t('form.timezone')}</FormLabel>
            <FormControl>
              <TimeZoneCombobox
                value={field.value}
                onChange={(zone) => zone && field.onChange(zone)}
                placeholder={t('form.timezonePlaceholder')}
                noResults={t('form.timezoneNoResults')}
                disabled={disabled}
              />
            </FormControl>
            <FormMessage />
          </FormItem>
        )}
      />
    </div>
  );
}

/** The next runs of the schedule being typed, as the server expects them. */
function Preview() {
  const t = useTranslations('crons');
  const { control } = useFormContext<MonitorFormData>();
  const [schedule, timezone, margin] = useWatch({
    control,
    name: ['schedule', 'timezone', 'checkin_margin'],
  });
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
