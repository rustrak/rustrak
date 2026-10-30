import type {
  CreateAlertIntegration,
  CreateAlertRule,
  UpdateAlertIntegration,
  UpdateAlertRule,
} from '@rustrak/client';
import { mutationOptions } from '@tanstack/react-query';
import { orThrow } from '@/shared/api/query';
import { api } from '@/shared/api/rustrak';
import { alertQueries } from './queries';

export const alertMutations = {
  createIntegration: () =>
    mutationOptions({
      mutationFn: (body: CreateAlertIntegration) =>
        orThrow(api().alertChannels.create({ body })),
      meta: {
        invalidates: [
          alertQueries.integrations().queryKey,
          ['invitations', 'email-status'],
        ],
      },
    }),
  // The integration is the variable, not a parameter: a list of them, or a
  // rule's channels tested together, is one mutation rather than one per row.
  updateIntegration: () =>
    mutationOptions({
      mutationFn: ({
        id,
        body,
      }: {
        id: number;
        body: UpdateAlertIntegration;
      }) => orThrow(api().alertChannels.update({ path: { id }, body })),
      // Enable/disable moves the invite form's delivery switch too.
      meta: {
        invalidates: [
          alertQueries.integrations().queryKey,
          ['invitations', 'email-status'],
        ],
      },
    }),
  /**
   * Rules in any project may route to the integration and lose it, and no key
   * narrower than every project's holds all of them.
   */
  deleteIntegration: () =>
    mutationOptions({
      mutationFn: (id: number) =>
        orThrow(api().alertChannels.delete({ path: { id } })),
      meta: {
        invalidates: [
          alertQueries.integrations().queryKey,
          ['invitations', 'email-status'],
          ['projects'],
        ],
      },
    }),
  /** Sends a sample alert through the integration; records nothing. */
  testIntegration: () =>
    mutationOptions({
      mutationFn: ({
        id,
        routingOverride,
      }: {
        id: number;
        routingOverride?: Record<string, unknown>;
      }) =>
        orThrow(
          api().alertChannels.test({
            path: { id },
            body:
              routingOverride === undefined
                ? undefined
                : { routing_override: routingOverride },
          }),
        ),
    }),

  createRule: (projectId: number) =>
    mutationOptions({
      mutationFn: (body: CreateAlertRule) =>
        orThrow(
          api().alertRules.create({ path: { project_id: projectId }, body }),
        ),
      meta: { invalidates: [alertQueries.rules(projectId)] },
    }),
  /** The rule is the variable: the rules table toggles any row with one. */
  updateRule: (projectId: number) =>
    mutationOptions({
      mutationFn: ({
        ruleId,
        body,
      }: {
        ruleId: number;
        body: UpdateAlertRule;
      }) =>
        orThrow(
          api().alertRules.update({
            path: { project_id: projectId, rule_id: ruleId },
            body,
          }),
        ),
      meta: { invalidates: [alertQueries.rules(projectId)] },
    }),
  /** The rule's deliveries go with it, so the history changes too. */
  deleteRule: (projectId: number, ruleId: number) =>
    mutationOptions({
      mutationFn: () =>
        orThrow(
          api().alertRules.delete({
            path: { project_id: projectId, rule_id: ruleId },
          }),
        ),
      // Dropped before the alerts refetch: the rule's page is still mounted,
      // and asking for a deleted rule can only answer 404.
      onSuccess: (_data, _variables, _result, { client }) =>
        client.removeQueries({
          queryKey: alertQueries.rule(projectId, ruleId).queryKey,
        }),
      meta: { invalidates: [alertQueries.all(projectId)] },
    }),
};
