import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import type { ApiError } from '@/shared/api/query';
import { describeError } from '@/shared/lib/error-copy';

/**
 * A failed write, as a toast: `title` says what failed, the description says
 * why in the app's own words (`describeError`), so an outage reads as one and
 * not as the redacted message the client carries for it.
 */
export function useToastFailure() {
  const t = useTranslations();
  return (title: string, error: ApiError) =>
    toast.error(title, { description: describeError(error.error, t) });
}
