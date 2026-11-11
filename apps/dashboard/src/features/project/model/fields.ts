import { z } from 'zod';
import type { Translate } from '@/shared/lib/error-copy';

const PROJECT_NAME_MIN_LENGTH = 2;
export const PROJECT_NAME_MAX_LENGTH = 100;

/**
 * The rules for a project name, shared by the create form and the general
 * settings form: two forms editing the same column must agree about what it
 * accepts.
 */
export function projectNameField(t: Translate) {
  return z
    .string()
    .trim()
    .min(
      PROJECT_NAME_MIN_LENGTH,
      t('fields.nameTooShort', { min: PROJECT_NAME_MIN_LENGTH }),
    )
    .max(
      PROJECT_NAME_MAX_LENGTH,
      t('fields.nameTooLong', { max: PROJECT_NAME_MAX_LENGTH }),
    );
}

/** The rules for a project slug, shared for the same reason. */
export function projectSlugField(t: Translate) {
  return z
    .string()
    .trim()
    .min(1, t('fields.slugEmpty'))
    .regex(/^[a-z0-9]+(?:[-_][a-z0-9]+)*$/, t('fields.slugChars'));
}

export function buildCreateProjectFormSchema(t: Translate) {
  return z.object({
    platform: z.string().min(1, t('platformRequired')),
    name: projectNameField(t),
    slug: projectSlugField(t),
  });
}

export type CreateProjectFormData = z.infer<
  ReturnType<typeof buildCreateProjectFormSchema>
>;

/**
 * Live preview slugifier, ported from Sentry's `utils/slugify`.
 *
 * Deliberately does NOT trim leading or trailing hyphens: doing so would make
 * it impossible to type "my-app", since the hyphen would vanish the moment it
 * is typed. The server slugifies again and is the real authority.
 */
export function slugifyPreview(value: string): string {
  return value
    .normalize('NFKD')
    .toLowerCase()
    .replace(/[^a-z0-9_\s-]/g, '')
    .replace(/[-\s]+/g, '-');
}
