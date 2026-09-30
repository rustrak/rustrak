import { Skeleton } from '@/shared/ui/components/shadcn/skeleton';

/**
 * Loading skeleton for the event detail page — mirrors the new layout:
 * elevated header band + workflow toolbar, then a two-pane body (main content
 * + right rail).
 */
export function EventLoading() {
  return (
    <div className="flex flex-col h-full bg-background">
      {/* Header band */}
      <header className="shrink-0 bg-card border-b">
        <div className="w-full px-4 md:px-8 py-3 space-y-2">
          <Skeleton className="h-3 w-40" />
          <div className="flex items-start justify-between gap-6">
            <Skeleton className="h-6 w-64 max-w-[60%]" />
            <div className="flex items-start gap-8 shrink-0">
              <div className="space-y-1 text-right">
                <Skeleton className="h-2.5 w-16 ml-auto" />
                <Skeleton className="h-6 w-10 ml-auto" />
              </div>
              <div className="space-y-1 text-right">
                <Skeleton className="h-2.5 w-16 ml-auto" />
                <Skeleton className="h-6 w-10 ml-auto" />
              </div>
            </div>
          </div>
          <Skeleton className="h-4 w-full max-w-2xl" />
          <Skeleton className="h-4 w-56" />
        </div>

        {/* Toolbar band */}
        <div className="border-t">
          <div className="w-full px-4 md:px-8 py-2 flex items-center justify-between gap-2">
            <div className="flex items-center gap-2">
              <Skeleton className="h-8 w-24" />
              <Skeleton className="h-8 w-24" />
              <Skeleton className="h-8 w-8" />
            </div>
            <Skeleton className="h-8 w-28" />
          </div>
        </div>
      </header>

      {/* Body */}
      <div className="flex-1 min-h-0 flex">
        <main className="flex-1 min-w-0 overflow-hidden">
          <div className="w-full px-4 md:px-8 py-5 space-y-5">
            {/* Trends */}
            <div className="grid grid-cols-1 lg:grid-cols-[1fr_340px] gap-4">
              <div className="rounded-lg border bg-card p-4 flex gap-5">
                <div className="shrink-0 space-y-3">
                  <div className="space-y-1">
                    <Skeleton className="h-2.5 w-12" />
                    <Skeleton className="h-6 w-10" />
                  </div>
                  <div className="space-y-1">
                    <Skeleton className="h-2.5 w-12" />
                    <Skeleton className="h-6 w-10" />
                  </div>
                </div>
                <Skeleton className="flex-1 h-[130px]" />
              </div>
              <div className="rounded-lg border bg-card p-4 space-y-3">
                <Skeleton className="h-2.5 w-10" />
                {Array.from({ length: 4 }).map((_, i) => (
                  <Skeleton key={i} className="h-2 w-full" />
                ))}
              </div>
            </div>

            {/* Event nav row */}
            <div className="space-y-3 border-b pb-3">
              <div className="flex items-center justify-between">
                <Skeleton className="h-4 w-32" />
                <Skeleton className="h-7 w-48" />
              </div>
              <Skeleton className="h-3 w-64" />
            </div>

            {/* Sections */}
            <div className="rounded-lg border bg-card p-4 space-y-5">
              {Array.from({ length: 3 }).map((_, i) => (
                <div key={i} className="space-y-3">
                  <Skeleton className="h-4 w-40" />
                  <Skeleton className="h-24 w-full" />
                </div>
              ))}
            </div>
          </div>
        </main>

        {/* Right rail */}
        <aside className="hidden lg:block w-[320px] shrink-0 border-l bg-card">
          <div className="p-4 space-y-2 border-b">
            <div className="flex justify-between">
              <Skeleton className="h-4 w-20" />
              <Skeleton className="h-4 w-24" />
            </div>
            <div className="flex justify-between">
              <Skeleton className="h-4 w-20" />
              <Skeleton className="h-4 w-24" />
            </div>
          </div>
          <div className="p-4 space-y-3">
            <Skeleton className="h-4 w-20" />
            <Skeleton className="h-20 w-full" />
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-3/4" />
          </div>
        </aside>
      </div>
    </div>
  );
}
