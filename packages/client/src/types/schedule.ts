import type { z } from 'zod';
import type {
  intervalUnitSchema,
  previewScheduleSchema,
  schedulePreviewSchema,
  scheduleSchema,
} from '../schemas/schedule.js';

export type IntervalUnit = z.infer<typeof intervalUnitSchema>;
export type Schedule = z.infer<typeof scheduleSchema>;
export type PreviewSchedule = z.input<typeof previewScheduleSchema>;
export type SchedulePreview = z.infer<typeof schedulePreviewSchema>;
