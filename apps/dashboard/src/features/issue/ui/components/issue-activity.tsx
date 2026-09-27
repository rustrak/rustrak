import type { ActivityEntry } from '@rustrak/client';
import { Loader2, MessageSquare } from 'lucide-react';
import { useState, useTransition } from 'react';
import { toast } from 'sonner';
import { useFormatter, useTranslations } from 'use-intl';
import { addIssueComment } from '@/features/issue/api/mutations';
import { invalidateIssues } from '@/features/issue/api/queries';
import { describeActivity, noteText } from '@/features/issue/lib/activity';
import { Button } from '@/shared/ui/components/shadcn/button';
import { Textarea } from '@/shared/ui/components/shadcn/textarea';

interface IssueActivityProps {
  projectId: number;
  issueId: string;
  activity: ActivityEntry[];
}

export function IssueActivity({
  projectId,
  issueId,
  activity,
}: IssueActivityProps) {
  const format = useFormatter();
  const t = useTranslations('issues');
  const [text, setText] = useState('');
  const [isPending, startTransition] = useTransition();

  const entries = [...activity].sort(
    (a, b) =>
      new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
  );

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const body = text.trim();
    if (!body) {
      return;
    }
    startTransition(async () => {
      const result = await addIssueComment(projectId, issueId, body);

      if (!result.success) {
        // Keep the text in the box: clearing it on a failure destroys what the
        // user wrote and leaves no trace that anything went wrong.
        toast.error(t('activity.commentFailed'), {
          description: result.error.message,
        });
        return;
      }

      setText('');
      void invalidateIssues(projectId);
    });
  };

  return (
    <div className="space-y-4">
      <h3 className="text-sm font-semibold">{t('activity.title')}</h3>

      <div className="space-y-5">
        <form onSubmit={handleSubmit} className="space-y-2">
          <Textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={t('activity.placeholder')}
            rows={3}
            disabled={isPending}
          />
          <div className="flex justify-end">
            <Button
              type="submit"
              size="sm"
              disabled={isPending || !text.trim()}
            >
              {isPending ? (
                <Loader2 className="mr-2 size-4 animate-spin" />
              ) : (
                <MessageSquare className="mr-2 size-4" />
              )}
              {t('activity.comment')}
            </Button>
          </div>
        </form>

        {entries.length === 0 ? (
          <p className="text-sm text-muted-foreground text-center py-6">
            {t('activity.empty')}
          </p>
        ) : (
          <ol className="space-y-4">
            {entries.map((entry) => {
              const isNote = entry.type === 'note' || entry.type === 'comment';
              return (
                <li key={entry.id} className="flex gap-3 text-sm">
                  <div className="mt-1.5 size-1.5 rounded-full bg-muted-foreground/50 shrink-0" />
                  <div className="min-w-0 flex-1">
                    {isNote ? (
                      <p className="whitespace-pre-wrap break-words">
                        {noteText(entry)}
                      </p>
                    ) : (
                      <p className="text-muted-foreground">
                        {describeActivity(entry, t)}
                      </p>
                    )}
                    <p className="text-xs text-muted-foreground/70 mt-0.5">
                      {format.relativeTime(new Date(entry.created_at))}
                    </p>
                  </div>
                </li>
              );
            })}
          </ol>
        )}
      </div>
    </div>
  );
}
