import {
  Button,
  CopyIcon,
  Dialog,
  DialogBody,
  DialogFooter,
  DialogHeader,
  Field,
  FieldHint,
  FieldLabel,
  Input,
  NewIcon,
  SelectedIcon,
} from '@rustrak/ui';
import { useState } from 'react';
import { useTranslations } from 'use-intl';

export const TOKEN_DESCRIPTION_MAX_LENGTH = 200;

/**
 * Creating a token, and then showing the one thing the server will not repeat
 * on demand in this shape: the full value, once.
 *
 * Two states in one dialog because they are two steps of one act. Closing
 * after the value is shown is what forgets it, which is why the caller gets
 * `onDone` rather than a plain `onOpenChange`.
 */
export function CreateTokenDialog({
  open,
  onOpenChange,
  newToken,
  isPending,
  onCreate,
  onDone,
  onCopyToken,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The full value, present only between creating it and closing the dialog. */
  newToken: string | null;
  isPending: boolean;
  onCreate: (description: string) => void;
  onDone: () => void;
  onCopyToken: () => Promise<boolean>;
}) {
  const t = useTranslations('tokens');
  const [description, setDescription] = useState('');
  const [copied, setCopied] = useState(false);

  const trimmed = description.trim();
  const isValid = trimmed.length <= TOKEN_DESCRIPTION_MAX_LENGTH;

  const create = () => {
    if (!isValid) return;
    onCreate(trimmed);
    setDescription('');
  };

  const copy = async () => {
    if (!(await onCopyToken())) return;
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <>
      <Button
        variant="primary"
        icon={NewIcon}
        onClick={() => onOpenChange(true)}
      >
        {t('create.trigger')}
      </Button>
      <Dialog open={open} onOpenChange={onOpenChange}>
        {newToken ? (
          <>
            <DialogHeader
              title={t('create.createdTitle')}
              description={t('create.createdDescription')}
            />
            <DialogBody>
              <div className="flex items-center gap-2 rounded-lg border border-border-subtle bg-surface p-3">
                <code className="flex-1 break-all font-mono text-mono text-fg">
                  {newToken}
                </code>
                <Button
                  variant="ghost"
                  size="sm"
                  icon={copied ? SelectedIcon : CopyIcon}
                  onClick={copy}
                  className="shrink-0"
                  aria-label={t('create.copyTokenLabel')}
                />
              </div>
              <p className="mt-2 text-hint text-fg-subtle">
                {t('create.copyAgainHint')}
              </p>
            </DialogBody>
            <DialogFooter>
              <Button variant="primary" onClick={onDone}>
                {t('create.done')}
              </Button>
            </DialogFooter>
          </>
        ) : (
          <>
            <DialogHeader
              title={t('create.title')}
              description={t('create.description')}
            />
            <DialogBody>
              <Field>
                <FieldLabel>{t('create.descriptionLabel')}</FieldLabel>
                <Input
                  placeholder={t('create.descriptionPlaceholder')}
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && create()}
                  // Deliberately above the limit: the field accepts a little
                  // overshoot so the counter can say what is wrong, rather
                  // than silently swallowing keystrokes at the boundary.
                  maxLength={TOKEN_DESCRIPTION_MAX_LENGTH + 10}
                  invalid={!isValid}
                />
                <FieldHint>
                  {t('create.charCount', {
                    count: trimmed.length,
                    max: TOKEN_DESCRIPTION_MAX_LENGTH,
                  })}
                </FieldHint>
                {!isValid && (
                  <p className="text-meta text-danger-fg">
                    {t('create.descriptionTooLong', {
                      max: TOKEN_DESCRIPTION_MAX_LENGTH,
                    })}
                  </p>
                )}
              </Field>
            </DialogBody>
            <DialogFooter>
              <Button
                variant="ghost"
                onClick={() => onOpenChange(false)}
                disabled={isPending}
              >
                {t('create.cancel')}
              </Button>
              <Button
                variant="primary"
                onClick={create}
                disabled={isPending || !isValid}
              >
                {isPending ? t('create.creating') : t('create.submit')}
              </Button>
            </DialogFooter>
          </>
        )}
      </Dialog>
    </>
  );
}
