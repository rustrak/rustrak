import {
  Button,
  EditIcon,
  Field,
  FieldError,
  FieldLabel,
  Input,
  UndoIcon,
} from '@rustrak/ui';
import { useTranslations } from 'use-intl';
import {
  PROJECT_NAME_MAX_LENGTH,
  slugifyPreview,
} from '@/features/project/model/fields';
import type { ProjectSlug } from '@/features/project/ui/hooks/use-project-slug';
import { cn } from '@/shared/lib/utils';
import {
  fieldErrorText,
  useFieldContext,
} from '@/shared/ui/components/app-form';

/**
 * The name, and the slug that follows it until the user says otherwise.
 *
 * These two are one control, not two: typing a name rewrites the slug, and the
 * pencil is what breaks that link. The state behind it lives in
 * `useProjectSlug` because the submit and the server-error path need the same
 * answer, so these components render it rather than own it. Each renders
 * inside its own `form.AppField`.
 */
export function NameField({
  slug,
  disabled,
}: {
  slug: ProjectSlug;
  disabled: boolean;
}) {
  const field = useFieldContext<string>();
  const t = useTranslations('projects');
  const error = fieldErrorText(field.state.meta.errors);

  return (
    <div className="pt-4">
      <Field name={field.name} invalid={!!error}>
        <FieldLabel className="text-label text-fg-muted">
          {t('fields.name')}
        </FieldLabel>
        <Input
          placeholder="my-application"
          autoComplete="off"
          disabled={disabled}
          maxLength={PROJECT_NAME_MAX_LENGTH}
          name={field.name}
          value={field.state.value}
          onBlur={field.handleBlur}
          invalid={!!error}
          onChange={(e) => {
            field.handleChange(e.target.value);
            slug.syncFromName(e.target.value);
          }}
        />
        {error ? <FieldError match>{error}</FieldError> : null}
      </Field>
    </div>
  );
}

export function SlugField({
  slug,
  disabled,
}: {
  slug: ProjectSlug;
  disabled: boolean;
}) {
  const field = useFieldContext<string>();
  const t = useTranslations('projects');
  const error = fieldErrorText(field.state.meta.errors);
  const toggleLabel = slug.isManual ? t('slug.reset') : t('slug.edit');

  return (
    <div className="pt-4">
      <Field name={field.name} invalid={!!error}>
        <FieldLabel className="text-label text-fg-muted">
          {t('fields.slug')}
        </FieldLabel>
        <div className="flex items-center gap-2">
          <Input
            placeholder="my-application"
            autoComplete="off"
            readOnly={!slug.isManual}
            aria-readonly={!slug.isManual}
            disabled={disabled}
            className={cn('font-mono', !slug.isManual && 'text-fg-subtle')}
            boxClassName="flex-1"
            name={field.name}
            value={field.state.value}
            onBlur={field.handleBlur}
            invalid={!!error}
            onChange={(e) => field.handleChange(slugifyPreview(e.target.value))}
          />
          <Button
            type="button"
            variant="ghost"
            icon={slug.isManual ? UndoIcon : EditIcon}
            disabled={disabled}
            aria-label={toggleLabel}
            title={toggleLabel}
            onClick={slug.toggle}
            className="shrink-0"
          />
        </div>
        {/* One line, not two: the hint and the error share the slot, so the
            panel never reflows. */}
        {error ? (
          <FieldError match className="mt-1.5">
            {error}
          </FieldError>
        ) : (
          <p className="mt-1.5 text-fg-meta text-hint">
            {slug.isManual ? t('slug.hintManual') : t('slug.hintAuto')}
          </p>
        )}
      </Field>
    </div>
  );
}
