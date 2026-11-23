import type { AlertType } from '@rustrak/client';

/**
 * The triggers a rule can fire on, in the order they are offered. A project
 * has at most one rule per trigger.
 */
export const alertTypes: { type: AlertType }[] = [
  { type: 'new_issue' },
  { type: 'regression' },
  { type: 'new_feedback' },
];
