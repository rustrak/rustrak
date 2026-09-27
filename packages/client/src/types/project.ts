import type { z } from 'zod';
import type {
  createProjectSchema,
  projectListStatsSchema,
  projectSchema,
  rateLimitsSchema,
  updateProjectSchema,
} from '../schemas/project.js';

/**
 * Project resource from the API
 */
export type Project = z.infer<typeof projectSchema>;

/**
 * Per-row aggregates attached when listing with `stats_period`
 */
export type ProjectListStats = z.infer<typeof projectListStatsSchema>;

/**
 * Request payload for creating a project
 */
export type CreateProject = z.infer<typeof createProjectSchema>;

/**
 * Request payload for updating a project
 */
export type UpdateProject = z.infer<typeof updateProjectSchema>;

/**
 * The server's per-project limits
 */
export type RateLimits = z.infer<typeof rateLimitsSchema>;
