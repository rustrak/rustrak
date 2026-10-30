import { Skeleton } from '@rustrak/ui';
import { PageLoading } from '@/shared/ui/components/page-loading';
import { RustrakWordmark } from '@/shared/ui/components/rustrak-wordmark';

/**
 * The card the public token screens (account linking, an invitation, an
 * assistant's consent) share, drawn while the read that decides what it says
 * is in flight. The read's failures are answers these pages render, so they
 * do not suspend: they show this while `useQuery` is pending.
 */
export function AuthScreenLoading({ wide = false }: { wide?: boolean }) {
  return (
    <PageLoading>
      <div className="min-h-screen bg-card flex items-center justify-center p-8 lg:p-12">
        <div
          className={`w-full ${wide ? 'max-w-[460px]' : 'max-w-[420px]'} space-y-10`}
        >
          <div className="flex items-center">
            <RustrakWordmark className="h-[22px] w-auto" />
          </div>
          <div className="space-y-6">
            <div className="space-y-2">
              <Skeleton className="h-9 w-64 max-w-full" />
              <Skeleton className="h-5 w-full" />
            </div>
            <Skeleton className="h-10 w-full" />
            <Skeleton className="h-10 w-full" />
          </div>
        </div>
      </div>
    </PageLoading>
  );
}
