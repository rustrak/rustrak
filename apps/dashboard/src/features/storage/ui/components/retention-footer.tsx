import type { CleanupCounts, ProjectStorage } from '@rustrak/client';
import {
  Button,
  Dialog,
  DialogBody,
  DialogClose,
  DialogFooter,
  DialogHeader,
} from '@rustrak/ui';
import { useState } from 'react';
import { useTranslations } from 'use-intl';
import {
  estimateSize,
  impactState,
  removedParts,
} from '@/features/storage/model/retention';
import { ImpactLine } from './impact-line';

/**
 * What a retention draft deletes, and its three ways out: cancel, save for
 * the next cleanup, or save and clean up now (asked once more).
 */
export function RetentionFooter({
  impact,
  onRetry,
  projects,
  isPending,
  onCancel,
  onSave,
}: {
  impact: CleanupCounts | null | 'failed';
  /** Asks for the count again after it failed. */
  onRetry: () => void;
  /** `null` until the storage figures are in. */
  projects: readonly ProjectStorage[] | null;
  isPending: boolean;
  onCancel: () => void;
  onSave: (cleanNow: boolean) => void;
}) {
  const t = useTranslations('storage');
  const [confirming, setConfirming] = useState(false);
  const known = impact === null || impact === 'failed' ? null : impact;
  const parts = known ? removedParts(known, t) : [];
  const size = known ? estimateSize(known, projects) : '';

  return (
    <>
      {/* Under the editor's own body, outside its scroll: what goes stays
          in sight while the periods above it change. */}
      <DialogBody className="flex-none">
        <ImpactLine
          state={impactState(impact)}
          sentence={
            parts.length > 0
              ? t('dialog.impact', { parts: parts.join(', '), size })
              : null
          }
          nothing={t('dialog.noImpact')}
          onRetry={onRetry}
        />
      </DialogBody>
      <DialogFooter>
        <Button variant="ghost" onClick={onCancel} disabled={isPending}>
          {t('cancel')}
        </Button>
        <Button
          onClick={() => setConfirming(true)}
          disabled={isPending || parts.length === 0}
        >
          {t('dialog.saveAndClean')}
        </Button>
        <Button
          variant="primary"
          onClick={() => onSave(false)}
          disabled={isPending}
          loading={isPending && !confirming}
        >
          {t('dialog.save')}
        </Button>
      </DialogFooter>

      <Dialog
        alert
        open={confirming}
        onOpenChange={(open) => !isPending && setConfirming(open)}
      >
        <DialogHeader
          dismissible={false}
          title={t('dialog.confirmTitle', { size })}
          description={t('dialog.confirmDescription', {
            parts: parts.join(', '),
          })}
        />
        <DialogFooter>
          <DialogClose
            disabled={isPending}
            render={<Button variant="secondary">{t('cancel')}</Button>}
          />
          <Button
            variant="danger-primary"
            onClick={() => onSave(true)}
            loading={isPending}
          >
            {t('dialog.confirmAction', { size })}
          </Button>
        </DialogFooter>
      </Dialog>
    </>
  );
}
