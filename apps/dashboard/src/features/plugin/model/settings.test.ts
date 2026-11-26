import type { SettingSpec } from '@rustrak/client';
import { describe, expect, it } from 'vitest';
import {
  canReset,
  fieldsAt,
  fieldValue,
  settingsPatch,
  settingText,
} from './settings';

const specs: SettingSpec[] = [
  {
    key: 'greeting',
    type: 'text',
    scope: 'instance',
    required: false,
    default: 'Hello',
  },
  { key: 'token', type: 'secret', scope: 'instance', required: false },
  {
    key: 'retentionDays',
    type: 'number',
    scope: 'project',
    required: false,
    default: 7,
  },
  { key: 'enabled', type: 'boolean', scope: 'project', required: false },
];

describe('fieldsAt', () => {
  it('shows every field for the instance, project ones as their default', () => {
    expect(fieldsAt(specs, 'instance').map((s) => s.key)).toEqual([
      'greeting',
      'token',
      'retentionDays',
      'enabled',
    ]);
  });

  it('shows only project fields in a project', () => {
    expect(fieldsAt(specs, 'project').map((s) => s.key)).toEqual([
      'retentionDays',
      'enabled',
    ]);
  });
});

describe('settingText', () => {
  const messages = {
    en: {
      settings: {
        greeting: { label: 'Greeting', description: 'What it says.' },
      },
    },
    es: { settings: { greeting: { label: 'Saludo' } } },
  };

  it("reads the reader's language, then English, then the key", () => {
    expect(settingText(messages, 'es', 'greeting', 'label')).toBe('Saludo');
    expect(settingText(messages, 'es', 'greeting', 'description')).toBe(
      'What it says.',
    );
    expect(settingText(messages, 'es', 'token', 'label')).toBe('token');
    expect(settingText(messages, 'es', 'token', 'description')).toBeUndefined();
  });
});

describe('settingsPatch', () => {
  it('sends only what changed, typed', () => {
    expect(
      settingsPatch(
        specs,
        { greeting: 'Hello', retentionDays: 7 },
        {
          greeting: 'Hello',
          retentionDays: '30',
          enabled: true,
        },
      ),
    ).toEqual({ retentionDays: 30, enabled: true });
  });

  it('clears an emptied field, so its default comes back', () => {
    expect(
      settingsPatch(specs, { greeting: 'Hola' }, { greeting: '' }),
    ).toEqual({ greeting: null });
  });

  it('offers a reset only on a saved switch or select not already reset', () => {
    const enabled = specs[3];
    expect(canReset(enabled, { enabled: false }, {})).toBe(true);
    expect(canReset(enabled, {}, {})).toBe(false);
    expect(canReset(enabled, { enabled: true }, { enabled: '' })).toBe(false);
    expect(canReset(specs[0], { greeting: 'Hola' }, {})).toBe(false);
  });

  it('shows a reset field as unset, so the inherited value shows', () => {
    expect(fieldValue('enabled', { enabled: true }, { enabled: '' })).toBe(
      undefined,
    );
    expect(fieldValue('enabled', { enabled: true }, {})).toBe(true);
    expect(fieldValue('enabled', { enabled: true }, { enabled: false })).toBe(
      false,
    );
  });

  it('clears a reset switch, so it inherits again', () => {
    expect(settingsPatch(specs, { enabled: false }, { enabled: '' })).toEqual({
      enabled: null,
    });
  });

  it('leaves a secret alone unless something is typed', () => {
    expect(settingsPatch(specs, {}, { token: '' })).toEqual({});
    expect(settingsPatch(specs, {}, { token: 'abc' })).toEqual({
      token: 'abc',
    });
  });
});
