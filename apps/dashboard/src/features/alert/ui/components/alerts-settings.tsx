import type { AlertIntegration, AlertRule, Project } from '@rustrak/client';
import { Link } from '@tanstack/react-router';
import { Bell, Plus } from 'lucide-react';
import { useState, useTransition } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import {
  deleteAlertRule,
  updateAlertRule,
} from '@/features/alert/api/mutations';
import { AlertRuleFormDialog } from '@/features/alert/ui/components/alert-rule-dialog/alert-rule-dialog';
import { AlertRulesTable } from '@/features/alert/ui/components/alert-rules-table';
import { ConfirmDeleteDialog } from '@/features/alert/ui/components/confirm-delete-dialog';
import { invalidateProject } from '@/shared/api/query-client';
import { Button } from '@/shared/ui/components/shadcn/button';

interface AlertsSettingsProps {
  project: Project;
  alertRules: AlertRule[];
  channels: AlertIntegration[];
}

export function AlertsSettings({
  project,
  alertRules,
  channels,
}: AlertsSettingsProps) {
  const t = useTranslations('alerts');
  const [isPending, startTransition] = useTransition();
  const [editingRule, setEditingRule] = useState<AlertRule | null>(null);
  const [deletingRule, setDeletingRule] = useState<AlertRule | null>(null);
  const [showAddForm, setShowAddForm] = useState(false);

  const enabledIntegrations = channels.filter((c) => c.is_enabled);

  const getIntegrationById = (id: number) => channels.find((c) => c.id === id);

  const handleToggleEnabled = (rule: AlertRule) => {
    startTransition(async () => {
      const result = await updateAlertRule(project.id, rule.id, {
        is_enabled: !rule.is_enabled,
      });

      if (!result.success) {
        // A switch in a table row, not a form field: the message is the only
        // place this can go.
        toast.error(t('settings.updateFailed'), {
          description: result.error.message,
        });
        return;
      }

      void invalidateProject(project.id);
    });
  };

  const handleDelete = () => {
    if (!deletingRule) return;
    startTransition(async () => {
      const result = await deleteAlertRule(project.id, deletingRule.id);

      if (!result.success) {
        toast.error(t('settings.deleteFailed'), {
          description: result.error.message,
        });
        return;
      }

      toast.success(t('settings.deleted'));
      setDeletingRule(null);
      void invalidateProject(project.id);
    });
  };

  return (
    <>
      <div className="mb-6 flex items-start justify-between gap-4 md:mb-8">
        <div>
          <h1 className="text-xl font-extrabold tracking-tight md:text-2xl">
            {t('settings.title')}
          </h1>
          <p className="mt-1 text-muted-foreground">
            {t('settings.description')}
          </p>
        </div>
        {enabledIntegrations.length > 0 && (
          <Button onClick={() => setShowAddForm(true)} className="shrink-0">
            <Plus className="mr-2 size-4" />
            {t('settings.addRule')}
          </Button>
        )}
      </div>

      {enabledIntegrations.length === 0 ? (
        <div className="rounded-lg border border-dashed py-16 text-center">
          <div className="mx-auto mb-4 flex size-12 items-center justify-center rounded-full bg-muted">
            <Bell className="size-5 text-muted-foreground" />
          </div>
          <p className="text-sm font-medium">
            {t('settings.noIntegrationsTitle')}
          </p>
          <p className="mt-1 text-xs text-muted-foreground">
            {t.rich('settings.noIntegrationsHint', {
              link: (chunks) => (
                <Link
                  to="/settings/integrations"
                  className="text-primary underline"
                >
                  {chunks}
                </Link>
              ),
            })}
          </p>
        </div>
      ) : alertRules.length === 0 ? (
        <div className="rounded-lg border border-dashed py-16 text-center">
          <div className="mx-auto mb-4 flex size-12 items-center justify-center rounded-full bg-muted">
            <Bell className="size-5 text-muted-foreground" />
          </div>
          <p className="text-sm font-medium">{t('settings.noRulesTitle')}</p>
          <p className="mt-1 text-xs text-muted-foreground">
            {t('settings.noRulesHint')}
          </p>
        </div>
      ) : (
        <AlertRulesTable
          rules={alertRules}
          getIntegrationById={getIntegrationById}
          disabled={isPending}
          onToggleEnabled={handleToggleEnabled}
          onEdit={setEditingRule}
          onDelete={setDeletingRule}
        />
      )}

      {/* Form dialog — create */}
      <AlertRuleFormDialog
        open={showAddForm}
        onOpenChange={(o) => !o && setShowAddForm(false)}
        projectId={project.id}
        integrations={enabledIntegrations}
        existingRule={null}
        existingRuleTypes={alertRules.map((r) => r.alert_type)}
        onSuccess={() => {
          setShowAddForm(false);
          void invalidateProject(project.id);
        }}
      />

      {/* Form dialog — edit */}
      <AlertRuleFormDialog
        open={!!editingRule}
        onOpenChange={(o) => !o && setEditingRule(null)}
        projectId={project.id}
        integrations={enabledIntegrations}
        existingRule={editingRule}
        existingRuleTypes={alertRules.map((r) => r.alert_type)}
        onSuccess={() => {
          setEditingRule(null);
          void invalidateProject(project.id);
        }}
      />

      <ConfirmDeleteDialog
        open={!!deletingRule}
        title={t('settings.deleteRuleTitle')}
        description={t('settings.deleteRuleDescription', {
          name: deletingRule?.name ?? '',
        })}
        isPending={isPending}
        onConfirm={handleDelete}
        onClose={() => setDeletingRule(null)}
      />
    </>
  );
}
