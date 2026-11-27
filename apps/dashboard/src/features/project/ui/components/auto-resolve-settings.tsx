import type { ProjectResponse } from '@rustrak/client';
import { Select, SelectItem, SettingRow, SettingSection } from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useId, useState } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { projectMutations } from '@/features/project/api/mutations';
import {
  resolveAgeChoices,
  resolveAgeDuration,
} from '@/shared/lib/resolve-age';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';

const OFF = 'off';

/**
 * Auto resolve (Sentry's "Auto Resolve"): resolve an issue once it has gone
 * this long without an event. Saved on choosing, like every other picker in
 * settings; the description warns that it cannot be undone.
 */
export function AutoResolveSettings({
  project,
  readOnly = false,
}: {
  project: ProjectResponse;
  /** Without `project:update`: shown, not changeable. */
  readOnly?: boolean;
}) {
  const t = useTranslations('projects.autoResolve');
  const toastFailure = useToastFailure();
  const id = useId();
  const stored =
    project.resolve_age === null ? OFF : String(project.resolve_age);
  // The project is the truth; only a choice still saving is held here, so
  // a change made elsewhere shows once the project is read again.
  const [pending, setPending] = useState<string | null>(null);
  const value = pending ?? stored;
  const update = useMutation(projectMutations.update(project.id));

  const label = (age: number) => {
    const { unit, count } = resolveAgeDuration(age);
    return t(unit, { count });
  };
  // A list, not the `items` record: an object puts numeric keys before
  // "off", so iterating it would list "Off" last.
  const options: [string, string][] = [
    [OFF, t('off')],
    ...resolveAgeChoices(project.resolve_age).map((age): [string, string] => [
      String(age),
      label(age),
    ]),
  ];

  const choose = (next: string) => {
    if (next === value) return;
    setPending(next);
    update.mutate(
      { resolve_age: next === OFF ? null : Number(next) },
      {
        onSuccess: () => toast.success(t('saved')),
        onError: (error) => toastFailure(t('saveFailed'), error),
        // The project read again by then carries the saved value.
        onSettled: () => setPending(null),
      },
    );
  };

  return (
    <SettingSection title={t('title')}>
      <SettingRow
        title={t('label')}
        description={t('description')}
        htmlFor={id}
      >
        <Select
          id={id}
          value={value}
          items={Object.fromEntries(options)}
          aria-label={t('label')}
          disabled={update.isPending || readOnly}
          onValueChange={choose}
        >
          {options.map(([age, text]) => (
            <SelectItem key={age} value={age}>
              {text}
            </SelectItem>
          ))}
        </Select>
      </SettingRow>
    </SettingSection>
  );
}
