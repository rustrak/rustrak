import { z } from 'zod';
import { globalRoleSchema } from './team.js';

/**
 * User schema - authenticated user information
 */
export const userSchema = z.object({
  id: z.number().int().positive(),
  email: z.string().email(),
  role: globalRoleSchema,
  is_admin: z.boolean(),
  /**
   * The dashboard language this user chose.
   *
   * `null` means they never chose one, which a consumer should treat as
   * "infer it" rather than as English. Optional as well as nullable so a
   * client stays compatible with a server predating the column.
   */
  language: z.string().nullable().optional(),
  /** The IANA timezone this user chose, `null` when they never chose one. */
  timezone: z.string().nullable().optional(),
});

/**
 * What a reader may change about how the dashboard presents itself.
 *
 * An absent key leaves the stored value alone; an explicit `null` clears it.
 */
export const updatePreferencesRequestSchema = z.object({
  language: z.string().nullable().optional(),
  timezone: z.string().nullable().optional(),
});

/**
 * Auth response schema - returned after login/register
 */
export const authResponseSchema = z.object({
  user: userSchema,
});

/**
 * Login result schema - includes user and session cookies for Server Actions
 */
export const loginResultSchema = z.object({
  user: userSchema,
  cookies: z.array(z.string()),
});

/**
 * Login request schema
 */
export const loginRequestSchema = z.object({
  email: z.string().email(),
  password: z.string().min(1),
});

/**
 * Register request schema
 */
export const registerRequestSchema = z.object({
  email: z.string().email(),
  password: z.string().min(1),
});

export const ssoConfigSchema = z.object({
  enabled: z.boolean(),
  provider_name: z.string().nullable(),
});

export const ssoStartSchema = z.object({
  authorization_url: z.string().url(),
});
