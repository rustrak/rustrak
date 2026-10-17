import type { Schedule } from '@rustrak/client';
import { useEffect, useState } from 'react';
import { createClient } from '@/shared/api/rustrak';

export interface SchedulePreviewState {
  /** ISO instants; `null` until an answer arrives for the current schedule. */
  runs: string[] | null;
  /** The server refused the schedule. */
  invalid: boolean;
  /** The request failed for any other reason. */
  failed: boolean;
  pending: boolean;
}

const IDLE: SchedulePreviewState = {
  runs: null,
  invalid: false,
  failed: false,
  pending: false,
};

/**
 * The next runs of the schedule being typed, asked of the server a moment
 * after the typing stops. The server's parser is the one that runs the
 * schedule (a monitor's expected check-ins, a job's runs), so the preview
 * cannot promise what it would not keep.
 *
 * An effect rather than an event handler: the request follows the form's
 * values, whichever field changed them.
 */
export function useSchedulePreview(
  schedule: Schedule | null,
  timezone: string,
): SchedulePreviewState {
  const [state, setState] = useState<SchedulePreviewState>(IDLE);
  // Compared by value: forms hand in a fresh object on every render.
  const key = schedule ? JSON.stringify({ schedule, timezone }) : null;

  useEffect(() => {
    if (key === null) {
      setState(IDLE);
      return;
    }
    let cancelled = false;
    setState((prev) => ({ ...prev, pending: true }));
    const timer = setTimeout(async () => {
      const client = await createClient();
      const result = await client.schedules.preview(JSON.parse(key));
      if (cancelled) return;
      if (result.success) {
        setState({ ...IDLE, runs: result.data.next_runs });
      } else {
        const invalid =
          result.error.kind === 'validation' ||
          result.error.kind === 'invalid_request';
        setState({ ...IDLE, invalid, failed: !invalid });
      }
    }, 300);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [key]);

  return state;
}
