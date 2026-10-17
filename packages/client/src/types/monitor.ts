import type { z } from 'zod';
import type {
  checkInSchema,
  checkInStatusSchema,
  createMonitorSchema,
  monitorConfigInputSchema,
  monitorConfigSchema,
  monitorEnvironmentSchema,
  monitorEnvironmentStatusSchema,
  monitorSchema,
  updateMonitorSchema,
} from '../schemas/monitor.js';

export type MonitorConfig = z.infer<typeof monitorConfigSchema>;
export type MonitorConfigInput = z.infer<typeof monitorConfigInputSchema>;
export type CheckInStatus = z.infer<typeof checkInStatusSchema>;
export type MonitorEnvironmentStatus = z.infer<
  typeof monitorEnvironmentStatusSchema
>;
export type CheckIn = z.infer<typeof checkInSchema>;
export type MonitorEnvironment = z.infer<typeof monitorEnvironmentSchema>;
export type Monitor = z.infer<typeof monitorSchema>;
export type CreateMonitor = z.input<typeof createMonitorSchema>;
export type UpdateMonitor = z.input<typeof updateMonitorSchema>;

/** Options for listing a monitor's check-ins. */
export interface ListCheckInsOptions {
  page?: number;
  per_page?: number;
  environment?: string;
}
