import { describe, expect, it } from 'vitest';
import { filledCredentials } from './credentials';

describe('filledCredentials', () => {
  it('keeps every field the user actually filled in', () => {
    expect(
      filledCredentials({ url: 'https://example.test', secret: 's3cret' }),
    ).toEqual({ url: 'https://example.test', secret: 's3cret' });
  });

  it('drops an empty string, which means "leave the stored value alone"', () => {
    expect(filledCredentials({ method: 'bot_token', token: '' })).toEqual({
      method: 'bot_token',
    });
  });

  it('treats whitespace as blank', () => {
    expect(filledCredentials({ token: '   ' })).toEqual({});
  });

  it('drops undefined, which an optional input reports as untouched', () => {
    expect(
      filledCredentials({ url: 'https://a.test', secret: undefined }),
    ).toEqual({ url: 'https://a.test' });
  });

  it('keeps a non-string value even when it is falsy', () => {
    expect(filledCredentials({ smtp_port: 0, verify_tls: false })).toEqual({
      smtp_port: 0,
      verify_tls: false,
    });
  });
});
