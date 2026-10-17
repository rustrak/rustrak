import { zodResolver } from '@hookform/resolvers/zod';
import type { AutoCleanup as AutoCleanupState } from '@rustrak/client';
import { CalendarClock, Loader2 } from 'lucide-react';
import { useMemo, useState, useTransition } from 'react';
import { useForm, useWatch } from 'react-hook-form';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import { updateAutoCleanup } from '@/features/storage/api/mutations';
import {
  type AutoCleanupFormData,
  autoCleanupFormSchema,
  runStatus,
  type StatusLine,
} from '@/features/storage/model/auto-cleanup-form';
import { applyServerFieldErrors } from '@/shared/lib/form-errors';
import { cn } from '@/shared/lib/utils';
import { ScheduleField } from '@/shared/ui/components/schedule-field';
import { SchedulePreview } from '@/shared/ui/components/schedule-preview';
import { Button } from '@/shared/ui/components/shadcn/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/shared/ui/components/shadcn/card';
import { Checkbox } from '@/shared/ui/components/shadcn/checkbox';
import {
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  FormRootError,
} from '@/shared/ui/components/shadcn/form';
import { Input } from '@/shared/ui/components/shadcn/input';
import { Switch } from '@/shared/ui/components/shadcn/switch';
import { TimeZoneCombobox } from '@/shared/ui/components/time-zone-combobox';

const LABEL =
  'text-xs font-bold uppercase tracking-widest text-muted-foreground';

const KINDS = [
  { name: 'include_events', label: 'type.events', hint: 'auto.eventsHint' },
  {
    name: 'include_transactions',
    label: 'type.transactions',
    hint: 'auto.transactionsHint',
  },
  { name: 'include_logs', label: 'type.logs', hint: 'auto.logsHint' },
] as const;

/**
 * Automatic cleanup: what to delete, how old, and when. Runs on the server's
 * scheduler, the same one that sweeps cron monitors, so its schedule and its
 * preview are the ones Crons uses.
 */
export function AutoCleanup({ initial }: { initial: AutoCleanupState }) {
  const t = useTranslations('storage');
  const globalT = useTranslations();
  const [state, setState] = useState(initial);
  const [isPending, startTransition] = useTransition();

  const schema = useMemo(() => autoCleanupFormSchema(t), [t]);
  const form = useForm<AutoCleanupFormData>({
    resolver: zodResolver(schema),
    defaultValues: initial.settings,
  });
  const [enabled, schedule, timezone] = useWatch({
    control: form.control,
    name: ['enabled', 'schedule', 'timezone'],
  });

  const onSubmit = (data: AutoCleanupFormData) =>
    startTransition(async () => {
      const result = await updateAutoCleanup(data);
      if (!result.success) {
        const applied = applyServerFieldErrors(form, result.error, {
          labels: { schedule: t('auto.scheduleLabel') },
          t: globalT,
        });
        if (applied.formLevel) {
          toast.error(t('auto.saveFailed'), { description: applied.formLevel });
        }
        return;
      }
      setState(result.data);
      form.reset(result.data.settings);
      toast.success(t('auto.saved'));
    });

  const filled =
    schedule.type === 'crontab'
      ? schedule.value.trim() !== ''
      : schedule.value >= 1;

  return (
    <Card>
      <Form {...form}>
        <form onSubmit={form.handleSubmit(onSubmit)}>
          <CardHeader>
            <div className="flex items-start justify-between gap-4">
              <div>
                <CardTitle className="flex items-center gap-2">
                  <CalendarClock className="size-4" />
                  {t('auto.title')}
                </CardTitle>
                <CardDescription className="mt-1.5">
                  {t('auto.description')}
                </CardDescription>
              </div>
              <FormField
                control={form.control}
                name="enabled"
                render={({ field }) => (
                  <label className="flex shrink-0 cursor-pointer items-center gap-2.5 text-sm font-medium">
                    {t('auto.enabled')}
                    <Switch
                      checked={field.value}
                      onCheckedChange={field.onChange}
                      disabled={isPending}
                    />
                  </label>
                )}
              />
            </div>
          </CardHeader>

          <CardContent
            className={cn(
              'space-y-6 pt-4 transition-opacity',
              !enabled && 'opacity-60',
            )}
          >
            <div className="grid gap-8 lg:grid-cols-[minmax(0,1fr)_18rem]">
              <div className="space-y-6">
                <FormField
                  control={form.control}
                  name="older_than_days"
                  render={({ field }) => (
                    <FormItem>
                      <FormLabel className={LABEL}>
                        {t('auto.olderThan')}
                      </FormLabel>
                      <div className="flex items-center gap-2">
                        <FormControl>
                          <Input
                            type="number"
                            inputMode="numeric"
                            min={1}
                            className="w-24 tabular-nums"
                            disabled={isPending}
                            name={field.name}
                            ref={field.ref}
                            onBlur={field.onBlur}
                            value={Number.isNaN(field.value) ? '' : field.value}
                            onChange={(e) =>
                              field.onChange(e.target.valueAsNumber)
                            }
                          />
                        </FormControl>
                        <span className="text-sm text-muted-foreground">
                          {t('auto.days', {
                            count: Number.isNaN(field.value) ? 2 : field.value,
                          })}
                        </span>
                      </div>
                      <FormMessage />
                    </FormItem>
                  )}
                />

                <fieldset className="space-y-3">
                  <legend className={cn(LABEL, 'mb-3')}>
                    {t('auto.dataLabel')}
                  </legend>
                  {KINDS.map((kind) => (
                    <FormField
                      key={kind.name}
                      control={form.control}
                      name={kind.name}
                      render={({ field }) => (
                        <label className="flex cursor-pointer select-none items-start gap-3">
                          <Checkbox
                            className="mt-0.5"
                            checked={field.value}
                            onCheckedChange={field.onChange}
                            disabled={isPending}
                          />
                          <span>
                            <span className="block text-sm font-medium">
                              {t(kind.label)}
                            </span>
                            <span className="block text-xs text-muted-foreground">
                              {t(kind.hint)}
                            </span>
                          </span>
                        </label>
                      )}
                    />
                  ))}
                  <FormField
                    control={form.control}
                    name="include_events"
                    render={() => <FormMessage />}
                  />
                </fieldset>

                <div className="space-y-4">
                  <FormField
                    control={form.control}
                    name="schedule"
                    render={({ field, fieldState }) => (
                      <ScheduleField
                        value={field.value}
                        onChange={field.onChange}
                        disabled={isPending}
                        error={fieldState.error?.message}
                      />
                    )}
                  />
                  <FormField
                    control={form.control}
                    name="timezone"
                    render={({ field }) => (
                      <FormItem className="max-w-xs">
                        <FormLabel className={LABEL}>
                          {t('auto.timezone')}
                        </FormLabel>
                        <TimeZoneCombobox
                          value={field.value}
                          onChange={(zone) => zone && field.onChange(zone)}
                          placeholder={t('auto.timezonePlaceholder')}
                          noResults={t('auto.timezoneNoResults')}
                          disabled={isPending}
                        />
                        <FormMessage />
                      </FormItem>
                    )}
                  />
                </div>
              </div>

              <aside className="self-start">
                <SchedulePreview
                  schedule={filled ? schedule : null}
                  timezone={timezone}
                />
              </aside>
            </div>

            <div className="flex flex-col-reverse gap-3 border-t pt-4 sm:flex-row sm:items-center sm:justify-between">
              <RunStatus state={state} />
              <Button type="submit" disabled={isPending}>
                {isPending && <Loader2 className="animate-spin" />}
                {isPending ? t('auto.saving') : t('auto.save')}
              </Button>
            </div>
            <FormRootError />
          </CardContent>
        </form>
      </Form>
    </Card>
  );
}

/** When it runs next and how the last run went, as saved on the server. */
function RunStatus({ state }: { state: AutoCleanupState }) {
  const t = useTranslations('storage');
  const format = useFormatter();
  const { next, last } = runStatus(state, t);
  const say = (line: StatusLine) =>
    t(line.key, {
      ...line.values,
      ...(line.at ? { when: format.relativeTime(new Date(line.at)) } : {}),
    });

  return (
    <div className="space-y-0.5 text-sm">
      <p className="font-medium">{say(next)}</p>
      {last && (
        <p
          className={cn(
            'text-muted-foreground',
            last.key === 'auto.lastRunFailed' && 'text-destructive',
          )}
        >
          {say(last)}
        </p>
      )}
    </div>
  );
}
