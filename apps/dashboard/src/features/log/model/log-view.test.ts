import { describe, expect, it } from 'vitest';
import { attributeGroups, matchParts, volumeBars } from './log-view';

describe('volumeBars', () => {
  it('folds fatal into error and trace into debug', () => {
    expect(
      volumeBars([
        {
          bucket: '2026-10-26T10:00:00Z',
          trace: 1,
          debug: 2,
          info: 3,
          warn: 4,
          error: 5,
          fatal: 6,
        },
      ]),
    ).toEqual([
      {
        t: Date.parse('2026-10-26T10:00:00Z'),
        debug: 3,
        info: 3,
        warn: 4,
        error: 11,
      },
    ]);
  });
});

describe('matchParts', () => {
  it('marks every match, ignoring case', () => {
    expect(matchParts('Payment failed, payment retried', 'PAYMENT')).toEqual([
      { text: 'Payment', match: true },
      { text: ' failed, ', match: false },
      { text: 'payment', match: true },
      { text: ' retried', match: false },
    ]);
  });

  it('takes the query literally and leaves blank queries alone', () => {
    expect(matchParts('a.b axb', '.')).toEqual([
      { text: 'a', match: false },
      { text: '.', match: true },
      { text: 'b axb', match: false },
    ]);
    expect(matchParts('text', '  ')).toEqual([{ text: 'text', match: false }]);
  });

  it('marks the match itself when a letter before it lowercases longer', () => {
    // `İ` lowercases to two code units, which shifted every later position.
    expect(matchParts('İ error', 'error')).toEqual([
      { text: 'İ ', match: false },
      { text: 'error', match: true },
    ]);
  });
});

describe('attributeGroups', () => {
  it('groups attributes by namespace, unwrapping typed values', () => {
    expect(
      attributeGroups({
        'sentry.sdk.name': { value: 'sentry.javascript', type: 'string' },
        'sentry.environment': { value: 'production', type: 'string' },
        'user.id': { value: 42, type: 'integer' },
        orderId: { value: 'A-7', type: 'string' },
        raw: [1, 2],
      }),
    ).toEqual([
      {
        name: '',
        attributes: [
          { key: 'orderId', value: 'A-7' },
          { key: 'raw', value: '[1,2]' },
        ],
      },
      {
        name: 'sentry',
        attributes: [
          { key: 'sentry.environment', value: 'production' },
          { key: 'sentry.sdk.name', value: 'sentry.javascript' },
        ],
      },
      { name: 'user', attributes: [{ key: 'user.id', value: '42' }] },
    ]);
  });

  it('has no groups without attributes', () => {
    expect(attributeGroups(null)).toEqual([]);
    expect(attributeGroups({})).toEqual([]);
  });
});
