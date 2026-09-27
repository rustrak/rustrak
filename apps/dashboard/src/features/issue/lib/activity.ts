import type { ActivityEntry } from '@rustrak/client';

function parseData(data: string): Record<string, unknown> {
  try {
    const parsed = JSON.parse(data);
    return parsed && typeof parsed === 'object' ? parsed : {};
  } catch {
    return {};
  }
}

/** The text body for a comment/note entry. */
export function noteText(entry: ActivityEntry): string {
  const text = parseData(entry.data).text;
  return typeof text === 'string' ? text : entry.data;
}

/** A human-readable description of a non-note activity entry. */
export function describeActivity(
  entry: ActivityEntry,
  t: (key: string, values?: Record<string, string | number>) => string,
): string {
  const d = parseData(entry.data);
  switch (entry.type) {
    case 'set_status': {
      const status = typeof d.status === 'string' ? d.status : '';
      switch (status) {
        case 'resolved':
          return t('activity.statusResolved');
        case 'unresolved':
          return t('activity.statusReopened');
        case 'ignored':
          return t('activity.statusMuted');
        default:
          return t('activity.statusChanged', { status });
      }
    }
    case 'set_priority':
      return t('activity.priorityChanged', {
        priority: String(d.priority ?? '—'),
      });
    case 'set_regression':
      return t('activity.regression');
    case 'first_seen':
      return t('activity.firstSeen');
    default:
      return entry.type.replace(/_/g, ' ');
  }
}
