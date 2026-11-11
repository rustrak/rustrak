import type { Schedule } from '@rustrak/client';
import { useQuery } from '@tanstack/react-query';
import { monitorQueries } from '@/features/monitor/api/queries';
import { useSettled } from '@/shared/ui/hooks/use-settled';

interface SchedulePreviewState {
  /** ISO instants; `null` until an answer arrives for the current schedule. */
  runs: string[] | null;
  /** The server refused the schedule. */
  invalid: boolean;
  /** The request failed for any other reason. */
  failed: boolean;
  pending: boolean;
}

/** Long enough to skip the keystrokes inside a field, short enough to feel live. */
const DEBOUNCE_MS = 300;

/**
 * The next runs of the schedule being typed, asked of the server a moment
 * after the typing stops. The server's parser is the one that runs the
 * schedule (a monitor's expected check-ins, a job's runs), so the preview
 * cannot promise what it would not keep.
 */
export function useSchedulePreview(
  schedule: Schedule | null,
  timezone: string,
): SchedulePreviewState {
  // Compared by value: forms hand in a fresh object on every render.
  const key = schedule ? JSON.stringify({ schedule, timezone }) : null;
  const settled = useSettled(key, DEBOUNCE_MS);
  const preview = useQuery({
    ...monitorQueries.schedulePreview(settled ? JSON.parse(settled) : {}),
    enabled: settled !== null,
  });

  if (key === null) {
    return { runs: null, invalid: false, failed: false, pending: false };
  }
  if (settled !== key || preview.isPending) {
    return { runs: null, invalid: false, failed: false, pending: true };
  }
  const refused =
    preview.error?.error.kind === 'validation' ||
    preview.error?.error.kind === 'invalid_request';
  return {
    runs: preview.data?.next_runs ?? null,
    invalid: refused,
    failed: preview.isError && !refused,
    pending: false,
  };
}
