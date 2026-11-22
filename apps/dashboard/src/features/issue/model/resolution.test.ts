import { describe, expect, it } from 'vitest';
import {
  regressionMessage,
  resolutionOf,
  resolvedInReleaseMessage,
} from './resolution';

describe('resolutionOf', () => {
  it('reads a known release', () => {
    expect(
      resolutionOf({
        status: 'resolved',
        status_details: { inRelease: 'app@1.0.4120' },
      }),
    ).toEqual({ kind: 'release', version: 'app@1.0.4120' });
  });

  it('reads the next release while it is not known', () => {
    expect(
      resolutionOf({
        status: 'resolved',
        status_details: { inNextRelease: true },
      }),
    ).toEqual({ kind: 'nextRelease' });
  });

  it('has none for a plain resolve or another status', () => {
    expect(resolutionOf({ status: 'resolved', status_details: {} })).toBeNull();
    expect(
      resolutionOf({
        status: 'unresolved',
        status_details: { inRelease: 'stale' },
      }),
    ).toBeNull();
  });
});

describe('resolvedInReleaseMessage', () => {
  it('says above the current release under semver', () => {
    expect(
      resolvedInReleaseMessage({
        version: '',
        current_release_version: 'app@1.0.4118',
      }),
    ).toEqual({
      key: 'activity.resolvedAfterRelease',
      values: { version: 'app@1.0.4118' },
    });
  });

  it('names a known release', () => {
    expect(resolvedInReleaseMessage({ version: 'b' })).toEqual({
      key: 'activity.resolvedInRelease',
      values: { version: 'b' },
    });
  });

  it('says the upcoming release while none is known', () => {
    expect(resolvedInReleaseMessage({ version: '' })).toEqual({
      key: 'activity.resolvedInNextRelease',
    });
  });
});

describe('regressionMessage', () => {
  it('names the release the event came from', () => {
    expect(regressionMessage({ version: '1.0.4120' })).toEqual({
      key: 'activity.regressionInRelease',
      values: { version: '1.0.4120' },
    });
  });

  it('falls back without one', () => {
    expect(regressionMessage({ status: 'unresolved' })).toEqual({
      key: 'activity.regression',
    });
  });
});
