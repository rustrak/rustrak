import type { SsoConfigResponse } from '@rustrak/client';

/**
 * How often the login page asks again for `/auth/sso/config`.
 *
 * Only a provider that is configured (it has a name) but not discovered yet is
 * worth waiting for: the server retries discovery in the background and the
 * button should appear once it lands. An instance without SSO answers
 * `enabled: false` forever, and polling it would be a request every five
 * seconds for nothing.
 */
export function ssoConfigPollInterval(
  config: SsoConfigResponse | undefined,
): number | false {
  return config?.enabled === false && config.provider_name != null
    ? 5000
    : false;
}
