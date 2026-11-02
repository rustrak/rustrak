import type { RustrakError } from '@rustrak/client';
import { IntegrationsIcon, Text } from '@rustrak/ui';
import { useTranslations } from 'use-intl';
import {
  describeError,
  errorGuidance,
  errorHeadline,
} from '@/shared/lib/error-copy';

/**
 * A failed read reported **in place**, inside a page that otherwise rendered.
 *
 * Its whole-viewport sibling is `OutageScreen`, which the root auth gate uses
 * because it has no header to sit under. The split is the point: a page that
 * loaded its header, its nav and three of its four panels must not be replaced
 * by the fourth panel's failure, and a screen with no chrome at all must not
 * render a small dashed card floating in an empty viewport.
 *
 * The surface is still not `/login`. A page that redirects on a network
 * failure sends a signed-in user to a login form that cannot help: logging in
 * issues the same request, which fails the same way, and they bounce again.
 *
 * Every line comes from `kind`. It used to assert an outage unconditionally --
 * heading "Rustrak is not responding", footer "reload once the API is back" --
 * which is right for `network` and `server_error` and wrong for the eleven
 * other kinds that reach it. A `forbidden` was told to wait for a server that
 * was already answering, and an `invalid_response` from a dashboard deployed
 * ahead of its API was told to reload, which can never fix a schema mismatch.
 */
export function ServiceUnavailable({
  error,
  title,
}: {
  error: RustrakError;
  /** Names the read that failed, for a page that does several. */
  title?: string;
}) {
  const t = useTranslations();
  const guidance = errorGuidance(error, t);

  return (
    <div className="p-6 md:p-8">
      <div className="flex flex-col items-center justify-center rounded-xl border border-border border-dashed px-6 py-14 text-center">
        <div className="mb-5 flex size-12 items-center justify-center rounded-full bg-surface-raised">
          <IntegrationsIcon
            className="size-5 text-fg-subtle"
            aria-hidden="true"
          />
        </div>
        <Text variant="section" render={<p />}>
          {title ?? errorHeadline(error, t)}
        </Text>
        <Text tone="subtle" render={<p />} className="mt-2 max-w-sm">
          {describeError(error, t)}
        </Text>
        {guidance ? (
          <Text
            variant="hint"
            tone="meta"
            render={<p />}
            className="mt-4 max-w-sm"
          >
            {guidance}
          </Text>
        ) : null}
      </div>
    </div>
  );
}
