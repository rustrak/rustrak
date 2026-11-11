import { Button, Input } from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { useState } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import { githubMutations } from '@/features/github/api/mutations';
import { SettingRow, SettingSection } from '@/shared/ui/components/setting-row';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';

/** The person's GitHub username, which is how assignees cross over. */
export function GitHubLoginCard({ login }: { login: string | null }) {
  const t = useTranslations('github');
  const toastFailure = useToastFailure();
  const [value, setValue] = useState(login ?? '');
  const [saved, setSaved] = useState(login ?? '');
  const { mutate, isPending } = useMutation({
    ...githubMutations.setMyLogin(),
    onSuccess: (data) => {
      setValue(data.login ?? '');
      setSaved(data.login ?? '');
      toast.success(t('account.saved'));
    },
    onError: (error) => toastFailure(t('account.saveFailed'), error),
  });

  const save = () => mutate(value.trim() || null);

  return (
    <SettingSection title={t('setup.you')}>
      <SettingRow
        title={t('account.title')}
        description={t('account.description')}
        htmlFor="github-login"
      >
        <form
          className="flex gap-2"
          onSubmit={(e) => {
            e.preventDefault();
            save();
          }}
        >
          <Input
            id="github-login"
            value={value}
            onChange={(e) => setValue(e.target.value)}
            placeholder="octocat"
            disabled={isPending}
            leading={<span className="text-fg-subtle">@</span>}
            boxClassName="min-w-0 flex-1"
          />
          <Button
            type="submit"
            variant="secondary"
            loading={isPending}
            disabled={value.trim() === saved}
          >
            {t('save')}
          </Button>
        </form>
      </SettingRow>
    </SettingSection>
  );
}
