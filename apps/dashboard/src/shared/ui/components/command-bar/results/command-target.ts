import type { CommandPath, ProjectPagePath } from '@/shared/config/commands';

/** Where a row leads: an instance-wide page, or one of a project's pages. */
export type CommandTarget =
  | { to: CommandPath }
  | { to: ProjectPagePath; params: { id: number } };
