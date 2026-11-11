import type {
  AlertIntegration,
  CreateAlertIntegration,
  RustrakError,
} from '@rustrak/client';
import { useMutation } from '@tanstack/react-query';
import { toast } from 'sonner';
import { alertMutations } from '@/features/alert/api/mutations';
import type { ApiError } from '@/shared/api/query';
import type { Translate } from '@/shared/lib/error-copy';
import {
  applyServerFieldErrors,
  type ServerFieldMap,
} from '@/shared/lib/form-errors';

/**
 * The two fields every integration dialog collects, whatever the provider.
 *
 * Everything else a form holds is provider-specific and reaches the server
 * folded into `credentials`, which is why the constraint stops here.
 */
export interface IntegrationFormBase {
  name: string;
  is_enabled: boolean;
}

/**
 * Already-translated copy, not message keys.
 *
 * Resolving at the call site is deliberate: it keeps every `t('slack.created')`
 * a string literal, which is the only form the message-keys architecture rule
 * can check. A key assembled here from `providerType` would type-check, render
 * correctly, and be invisible to the rule that catches it going stale.
 */
export interface IntegrationSubmitMessages {
  readonly saveFailed: string;
  readonly created: string;
  readonly updated: string;
}

export interface UseIntegrationSubmitOptions<T extends IntegrationFormBase> {
  readonly form: Parameters<typeof applyServerFieldErrors>[0];
  /** `null` while creating; the record being edited otherwise. */
  readonly existingIntegration: AlertIntegration | null;
  readonly providerType: CreateAlertIntegration['provider_type'];
  /** The flat inputs, folded into the one opaque object the server stores. */
  readonly credentials: (data: T) => Record<string, unknown>;
  /** Server dot path -> the name this form registers. */
  readonly fieldMap: ServerFieldMap;
  /** Human label per form field, so error copy reads in the user's words. */
  readonly labels: Readonly<Record<string, string>>;
  readonly messages: IntegrationSubmitMessages;
  /** The global translator, which `applyServerFieldErrors` needs for its copy. */
  readonly t: Translate;
  /** Run after a save the server accepted, once the list is read again. */
  readonly onSaved: () => void;
}

export interface IntegrationSubmit<T extends IntegrationFormBase> {
  readonly submit: (data: T) => void;
  readonly isPending: boolean;
}

/**
 * Create-or-update for one alert integration, with its failure handling.
 *
 * The webhook, email and Slack dialogs differ only in which inputs they render
 * and what those inputs are called; the save itself is one sequence for all
 * three: fold the credentials, post to create or update, mark the offending
 * inputs on rejection, and on success announce it, close and refresh. That
 * sequence lived three times, and the copies had already drifted apart in how
 * they decided a credential was blank.
 */
export function useIntegrationSubmit<T extends IntegrationFormBase>(
  options: UseIntegrationSubmitOptions<T>,
): IntegrationSubmit<T> {
  const create = useMutation(alertMutations.createIntegration());
  const update = useMutation(alertMutations.updateIntegration());

  // Update where there is something to update, create otherwise.
  // `provider_type` is only sent on create: it is what the integration *is*,
  // and the update endpoint does not accept a change of it.
  const submit = (data: T) => {
    const { existingIntegration, providerType, credentials } = options;
    const body = {
      name: data.name,
      credentials: credentials(data),
      is_enabled: data.is_enabled,
    };
    const callbacks = {
      onError: (error: ApiError) => reportFailure(options, error.error),
      onSuccess: () => {
        const { messages, onSaved } = options;
        toast.success(
          existingIntegration ? messages.updated : messages.created,
        );
        onSaved();
      },
    };
    if (existingIntegration) {
      update.mutate({ id: existingIntegration.id, body }, callbacks);
    } else {
      create.mutate({ ...body, provider_type: providerType }, callbacks);
    }
  };

  return { submit, isPending: create.isPending || update.isPending };
}

/** Put the rejection where the user can act on it. */
function reportFailure<T extends IntegrationFormBase>(
  options: UseIntegrationSubmitOptions<T>,
  error: RustrakError,
): void {
  const { form, fieldMap, labels, messages, t } = options;
  const applied = applyServerFieldErrors(form, error, {
    map: fieldMap,
    labels,
    t,
  });

  // A toast beside an input that already carries the reason is noise. Only a
  // failure that named no field of this dialog needs one.
  if (applied.formLevel) {
    toast.error(messages.saveFailed, { description: applied.formLevel });
  }
}
