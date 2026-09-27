import type { SourceMapGcResult } from '@rustrak/client';
import { FileCode2 } from 'lucide-react';
import { useState, useTransition } from 'react';
import { toast } from 'sonner';
import { useTranslations } from 'use-intl';
import {
  gcStorageSourceMaps,
  previewStorageSourceMapGc,
} from '@/features/storage/api/mutations';
import { invalidateAll } from '@/shared/api/query-client';
import { formatBytes } from '@/shared/lib/utils';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogTrigger,
} from '@/shared/ui/components/shadcn/alert-dialog';
import { Button } from '@/shared/ui/components/shadcn/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/shared/ui/components/shadcn/card';

export function SourceMapGc() {
  const t = useTranslations('storage');
  const [isPending, startTransition] = useTransition();
  const [preview, setPreview] = useState<SourceMapGcResult | null>(null);

  const handlePreview = () => {
    startTransition(async () => {
      const result = await previewStorageSourceMapGc();

      if (!result.success) {
        toast.error(t('toasts.sourceMapPreviewFailed'), {
          description: result.error.message,
        });
        return;
      }

      setPreview(result.data);
    });
  };

  const handleGc = () => {
    startTransition(async () => {
      const result = await gcStorageSourceMaps();

      if (!result.success) {
        toast.error(t('toasts.gcFailed'), {
          description: result.error.message,
        });
        return;
      }

      toast.success(
        t('toasts.sourceMapsRemoved', {
          count: result.data.files_removed,
          freed: formatBytes(result.data.bytes_freed),
        }),
      );
      setPreview(null);
      void invalidateAll();
    });
  };

  const nothingToClean = preview !== null && preview.files_removed === 0;

  return (
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <FileCode2 className="size-4" />
          {t('sourceMaps.title')}
        </CardTitle>
        <CardDescription>{t('sourceMaps.description')}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <Button
          type="button"
          variant="outline"
          onClick={handlePreview}
          disabled={isPending}
        >
          {isPending ? t('working') : t('preview')}
        </Button>

        {preview !== null && (
          <div className="rounded-md border bg-muted/40 p-4 text-sm">
            {nothingToClean ? (
              <p className="text-muted-foreground">
                {t('sourceMaps.nothingToClean')}
              </p>
            ) : (
              <>
                <p className="font-semibold mb-1">
                  {t('sourceMaps.wouldRemove')}
                </p>
                <ul className="text-muted-foreground space-y-0.5 tabular-nums">
                  <li>
                    {t('sourceMaps.fileCount', {
                      count: preview.files_removed,
                    })}
                  </li>
                  <li>
                    {t('sourceMaps.freed', {
                      bytes: formatBytes(preview.bytes_freed),
                    })}
                  </li>
                </ul>

                <AlertDialog>
                  <AlertDialogTrigger
                    render={
                      <Button
                        variant="destructive"
                        className="mt-3"
                        disabled={isPending}
                      />
                    }
                  >
                    <FileCode2 className="mr-2 size-4" />
                    {t('sourceMaps.removeAction')}
                  </AlertDialogTrigger>
                  <AlertDialogContent>
                    <AlertDialogHeader>
                      <AlertDialogTitle>
                        {t('sourceMaps.removeTitle', {
                          count: preview.files_removed,
                        })}
                      </AlertDialogTitle>
                      <AlertDialogDescription>
                        {t('sourceMaps.removeDescription', {
                          count: preview.files_removed,
                          bytes: formatBytes(preview.bytes_freed),
                        })}
                      </AlertDialogDescription>
                    </AlertDialogHeader>
                    <AlertDialogFooter>
                      <AlertDialogCancel disabled={isPending}>
                        {t('cancel')}
                      </AlertDialogCancel>
                      <AlertDialogAction
                        onClick={handleGc}
                        disabled={isPending}
                        className="bg-destructive text-white hover:bg-destructive/90"
                      >
                        {isPending ? t('removing') : t('remove')}
                      </AlertDialogAction>
                    </AlertDialogFooter>
                  </AlertDialogContent>
                </AlertDialog>
              </>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
