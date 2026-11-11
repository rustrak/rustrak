import type { FieldErrorCode, RustrakError } from '@rustrak/client';
import type { ValidationErrorMap } from '@tanstack/react-form';
import { describeError, type Translate } from '@/shared/lib/error-copy';

/**
 * Server errors are written under the key the schema validates on
 * (`revalidateLogic` + `validators.onDynamic`), so the next validation run, the
 * first edit after a submit or the next submit, replaces them exactly as it
 * replaces a schema error. Under `onServer` nothing would ever clear them, and
 * a field holding one blocks every later submit.
 */
const SERVER_ERROR_KEY = 'onDynamic';

/** The two parts of a TanStack `FormApi` this module touches. */
interface ServerErrorTarget {
  readonly fieldInfo: Readonly<
    Record<string, { instance: unknown } | undefined>
  >;
  setErrorMap(errorMap: {
    onDynamic: { form?: string; fields: Record<string, string> };
  }): void;
}

/**
 * Write server errors onto the form: `form` for the form-level slot, `fields`
 * keyed by field name. Writing `{}` clears both.
 */
export function setServerErrors(
  form: ServerErrorTarget,
  errors: { form?: string; fields?: Record<string, string> },
): void {
  form.setErrorMap({
    [SERVER_ERROR_KEY]: { form: errors.form, fields: errors.fields ?? {} },
  });
  if (errors.fields && Object.keys(errors.fields).length > 0) {
    focusFirstInvalid();
  }
}

/** Once the errors have rendered, take the reader to the first one. */
export function focusFirstInvalid(root?: ParentNode): void {
  globalThis.requestAnimationFrame?.(() =>
    (root ?? document)
      .querySelector<HTMLElement>('form [aria-invalid="true"]')
      ?.focus(),
  );
}

/** The form-level server message, if one stands. */
export function serverFormError(
  errorMap: ValidationErrorMap,
): string | undefined {
  const value = errorMap[SERVER_ERROR_KEY];
  return typeof value === 'string' ? value : undefined;
}

/**
 * Maps a server field path onto the name this form registers.
 *
 * Needed whenever the request body's shape is not the form's shape. The alert
 * integration dialogs are the reason it exists: they render flat inputs (`url`,
 * `secret`, `smtp_host`) and post them nested inside one opaque `credentials`
 * object, so the server can only ever name `credentials.url`. Without a map
 * that path matches nothing and the message ends up in the form-level slot,
 * which is correct but strictly less useful than marking the input.
 */
export type ServerFieldMap = Readonly<Record<string, string>>;

export interface ApplyServerFieldErrorsOptions {
  /** Server dot path -> the name this form registers. */
  readonly map?: ServerFieldMap;
  /**
   * Human label per **form** field name, used to build the copy.
   * `slug` -> `'Slug'` turns `already_exists` into "Slug is already taken."
   */
  readonly labels?: Readonly<Record<string, string>>;
  /** Resolves the copy sentences. */
  readonly t: Translate;
  /**
   * Copy for a code that means something narrower on one **form** field:
   * `incorrect` on a current password is a wrong password.
   */
  readonly messages?: Readonly<
    Record<string, Partial<Record<FieldErrorCode, string>>>
  >;
}

export interface AppliedServerFieldErrors {
  /** Form field names that were marked on their own input. */
  readonly marked: readonly string[];
  /**
   * The message placed on the form-level slot, or `null` when every part
   * of the failure landed on an input. A caller that wants a toast should raise
   * one only when this is non-null: a toast beside a marked input is noise.
   */
  readonly formLevel: string | null;
}

/**
 * Put a `RustrakError` onto a TanStack form, field by field.
 *
 * A server field path is marked only when that field is mounted right now:
 * an error on a name with no input on screen (the Slack dialog's other tab,
 * a path the form never renders) would block the next submit with nothing
 * explaining why. Everything else goes to the form-level slot.
 *
 * Copy comes from `(field, code)`, never from `message`, which is what makes it
 * translatable. The exception is `code: 'custom'`, where the code set cannot
 * express the reason and the server's own text is rendered verbatim.
 */
export function applyServerFieldErrors(
  form: ServerErrorTarget,
  error: RustrakError,
  options: ApplyServerFieldErrorsOptions,
): AppliedServerFieldErrors {
  const { map = {}, labels = {}, t, messages = {} } = options;
  const fields = 'fields' in error ? (error.fields ?? []) : [];

  const marked: Record<string, string> = {};
  const unattributed: string[] = [];

  for (const fieldError of fields) {
    const name = map[fieldError.field] ?? fieldError.field;
    const message =
      messages[name]?.[fieldError.code] ??
      copyFor(fieldError.code, labels[name], fieldError.message, t);

    // One message per input: a second reason for the same field would
    // replace the first, so it goes to the form-level slot instead.
    if (!form.fieldInfo[name]?.instance || name in marked) {
      unattributed.push(message);
      continue;
    }
    marked[name] = message;
  }

  if (fields.length === 0) {
    unattributed.push(describeError(error, t));
  }

  const formLevel = unattributed.length > 0 ? unattributed.join(' ') : null;
  setServerErrors(form, { form: formLevel ?? undefined, fields: marked });

  return { marked: Object.keys(marked), formLevel };
}

/**
 * The app's own copy for one `(field, code)` pair.
 *
 * `label` is the form's word for the input, so the sentence reads as the user's
 * own vocabulary rather than the API's.
 */
function copyFor(
  code: FieldErrorCode,
  label: string | undefined,
  serverMessage: string | undefined,
  t: Translate,
): string {
  if (code === 'custom') {
    // The one code whose meaning lives in the message. The server sends it
    // precisely because the code set could not express the reason.
    return serverMessage ?? t('formErrors.rejected');
  }

  const subject = label ?? t('formErrors.defaultSubject');

  switch (code) {
    case 'required':
      return t('formErrors.required', { subject });
    case 'invalid':
      return t('formErrors.invalid', { subject });
    case 'already_exists':
      return t('formErrors.alreadyExists', { subject });
    case 'too_short':
      return t('formErrors.tooShort', { subject });
    case 'too_long':
      return t('formErrors.tooLong', { subject });
    case 'incorrect':
      return t('formErrors.incorrect', { subject });
  }
}
