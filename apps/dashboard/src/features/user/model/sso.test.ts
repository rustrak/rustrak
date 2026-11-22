import { describe, expect, it } from 'vitest';
import { ssoConfigPollInterval } from './sso';

describe('ssoConfigPollInterval', () => {
  it('polls while a configured provider is not discovered yet', () => {
    expect(
      ssoConfigPollInterval({
        enabled: false,
        provider_name: 'Okta',
        password_login_enabled: true,
      }),
    ).toBe(5000);
  });

  it('never polls when SSO is not configured', () => {
    expect(
      ssoConfigPollInterval({
        enabled: false,
        provider_name: null,
        password_login_enabled: true,
      }),
    ).toBe(false);
  });

  it('stops once the provider is discovered', () => {
    expect(
      ssoConfigPollInterval({
        enabled: true,
        provider_name: 'Okta',
        password_login_enabled: true,
      }),
    ).toBe(false);
  });

  it('does not poll before the first answer', () => {
    expect(ssoConfigPollInterval(undefined)).toBe(false);
  });
});
