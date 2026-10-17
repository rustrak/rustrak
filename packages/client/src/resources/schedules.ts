import type { RustrakError } from '../errors.js';
import type { Result } from '../result.js';
import {
  previewScheduleSchema,
  schedulePreviewSchema,
} from '../schemas/schedule.js';
import type { PreviewSchedule, SchedulePreview } from '../types/schedule.js';
import { BaseResource } from './base.js';

/**
 * Schedules API resource. The server parses every schedule, so a client that
 * shows "next runs" asks here instead of shipping a cron parser that could
 * disagree with the one that actually runs them.
 */
export class SchedulesResource extends BaseResource {
  /** Validates a schedule and lists its next five runs. */
  async preview(
    input: PreviewSchedule,
  ): Promise<Result<SchedulePreview, RustrakError>> {
    const validated = this.validateInput(input, previewScheduleSchema);
    if (!validated.success) {
      return validated;
    }
    return this.request(
      () => this.http.post('api/schedules/preview', { json: validated.data }),
      schedulePreviewSchema,
    );
  }
}
