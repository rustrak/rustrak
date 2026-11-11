import type { RustrakError } from '@rustrak/client';
import { FieldApi, FormApi, revalidateLogic } from '@tanstack/react-form';
import { describe, expect, it } from 'vitest';
import { z } from 'zod';
import { applyServerFieldErrors, serverFormError } from './form-errors';

const t = (key: string, values?: Record<string, string | number>) =>
  values ? `${key}:${JSON.stringify(values)}` : key;

function setup(mounted: string[]) {
  const form = new FormApi({
    defaultValues: { slug: '', name: '', token: '' },
    validationLogic: revalidateLogic(),
    validators: {
      onDynamic: z.object({
        slug: z.string(),
        name: z.string(),
        token: z.string(),
      }),
    },
  });
  form.mount();
  const fields = Object.fromEntries(
    mounted.map((name) => {
      const field = new FieldApi({ form, name: name as 'slug' });
      field.mount();
      return [name, field];
    }),
  );
  return { form, fields };
}

const validation = (
  fields: { field: string; code: string; message?: string }[],
): RustrakError =>
  ({ kind: 'validation', message: 'bad', fields }) as unknown as RustrakError;

const errorsOf = (form: ReturnType<typeof setup>['form'], name: string) =>
  form.getFieldMeta(name as 'slug')?.errors ?? [];

describe('applyServerFieldErrors', () => {
  it('marks a mounted field and leaves the form-level slot empty', () => {
    const { form } = setup(['slug']);
    const result = applyServerFieldErrors(
      form,
      validation([{ field: 'slug', code: 'already_exists' }]),
      { labels: { slug: 'Slug' }, t },
    );

    expect(result).toEqual({ marked: ['slug'], formLevel: null });
    expect(errorsOf(form, 'slug')).toEqual([
      'formErrors.alreadyExists:{"subject":"Slug"}',
    ]);
    expect(serverFormError(form.state.errorMap)).toBeUndefined();
  });

  it('says a value is incorrect, not malformed', () => {
    const { form } = setup(['current_password']);
    applyServerFieldErrors(
      form,
      validation([{ field: 'current_password', code: 'incorrect' }]),
      { labels: { current_password: 'Current password' }, t },
    );

    expect(errorsOf(form, 'current_password')).toEqual([
      'formErrors.incorrect:{"subject":"Current password"}',
    ]);
  });

  it('says what a code means for one field when the form knows better', () => {
    const { form } = setup(['token']);
    applyServerFieldErrors(
      form,
      validation([{ field: 'token', code: 'invalid' }]),
      { t, messages: { token: { invalid: 'Wrong token' } } },
    );

    expect(errorsOf(form, 'token')).toEqual(['Wrong token']);
  });

  it('sends a field that is not mounted to the form-level slot', () => {
    const { form } = setup(['slug']);
    const result = applyServerFieldErrors(
      form,
      validation([
        { field: 'credentials.token', code: 'custom', message: 'Nope' },
      ]),
      { map: { 'credentials.token': 'token' }, t },
    );

    expect(result).toEqual({ marked: [], formLevel: 'Nope' });
    expect(serverFormError(form.state.errorMap)).toBe('Nope');
  });

  it('keeps a second reason for the same field at form level', () => {
    const { form } = setup(['slug']);
    const result = applyServerFieldErrors(
      form,
      validation([
        { field: 'slug', code: 'custom', message: 'One' },
        { field: 'slug', code: 'custom', message: 'Two' },
      ]),
      { t },
    );

    expect(result).toEqual({ marked: ['slug'], formLevel: 'Two' });
  });

  it('describes a failure that names no field', () => {
    const { form } = setup(['slug']);
    const error = { kind: 'network', reason: 'timeout' } as RustrakError;
    const result = applyServerFieldErrors(form, error, { t });

    expect(result.formLevel).toBe('error.describe.networkTimeout');
  });

  it('clears a server error once the user edits after submitting', async () => {
    const { form, fields } = setup(['slug', 'name']);
    await form.handleSubmit();
    applyServerFieldErrors(
      form,
      validation([{ field: 'slug', code: 'already_exists' }]),
      { t },
    );
    expect(errorsOf(form, 'slug')).toHaveLength(1);

    fields.slug.handleChange('other');

    expect(errorsOf(form, 'slug')).toEqual([]);
    expect(form.state.canSubmit).toBe(true);
  });
});
