import { Link } from '@tanstack/react-router';
import type { ReactNode } from 'react';
import { useTranslations } from 'use-intl';
import { RustrakWordmark } from '@/shared/ui/components/rustrak-wordmark';

/**
 * The full-viewport failure screen.
 *
 * Deliberately the same shape as `/login`: brand panel on the left,
 * content on the right against `bg-card`. Both are pages the user meets with
 * no header and no navigation, so they are the two places the app has to
 * introduce itself rather than assume the chrome already did.
 *
 * This is **not** for a failure inside a page that otherwise rendered. A tile
 * that could not load, or one panel of a release page, keeps the in-place card
 * in `service-unavailable.tsx`: handing the whole viewport to a failed
 * sub-request would hide the parts that did load.
 */
export function ErrorScreen({
  headline,
  description,
  guidance,
  detail,
  actions,
  brandStatement,
  brandDescription,
}: {
  /** The one-line answer to "what happened", on the content side. */
  headline: string;
  /** One sentence saying what went wrong. */
  description: string;
  /**
   * The large line on the brand panel. Defaults to the reassurance an outage
   * needs; a 404 overrides it, because nothing failed there and telling
   * someone their data is safe when they simply mistyped a URL is a non
   * sequitur.
   */
  brandStatement?: string;
  /** The paragraph under {@link brandStatement}. */
  brandDescription?: string;
  /** What the reader can do, when there is anything useful to say. */
  guidance?: string | null;
  /** Monospace footnote: an error id, anything diagnostic. */
  detail?: ReactNode;
  /** Buttons. */
  actions?: ReactNode;
}) {
  const t = useTranslations('errorScreen');
  const statement = brandStatement ?? t('brandStatement');
  const statementDescription = brandDescription ?? t('brandDescription');

  return (
    <div className="min-h-screen flex">
      {/* Left panel, decorative. Hidden below lg, exactly like the login. */}
      <div className="hidden lg:flex lg:w-1/2 bg-background flex-col justify-between p-12 relative overflow-hidden">
        <div className="absolute inset-0 bg-[radial-gradient(circle_at_top_right,_hsl(var(--card)),_transparent_50%)]" />
        <div className="absolute bottom-0 left-0 right-0 h-32 bg-gradient-to-t from-background to-transparent z-10" />

        <Link to="/" className="relative z-20 flex items-center w-fit">
          <RustrakWordmark className="h-[22px] w-auto" />
        </Link>

        {/* Brand furniture, not error copy. The right half carries what went
            wrong; repeating it here at 7xl would say the same thing twice on
            every viewport wide enough to show both. */}
        <div className="relative z-20 max-w-xl">
          <h2 className="text-6xl xl:text-7xl font-extrabold tracking-tighter leading-[1.05] mb-8 text-balance">
            {statement}
            <span className="text-primary">.</span>
          </h2>
          <p className="text-muted-foreground text-lg font-medium leading-relaxed max-w-md">
            {statementDescription}
          </p>
        </div>

        {/* No version here either, and for a sharper reason than on the
            login: a 404 is the one page an unauthenticated stranger can reach
            on any path they like, so a number printed here is a number handed
            to anyone who types a wrong URL. */}
        <div className="relative z-20 flex items-end text-xs text-muted-foreground font-mono">
          <p>&copy; {new Date().getFullYear()} Rustrak</p>
        </div>
      </div>

      {/* Right panel, the actionable half. */}
      <div className="w-full lg:w-1/2 bg-card flex items-center justify-center p-8 lg:p-12">
        <div className="w-full max-w-[420px] space-y-8">
          {/* Brand, for the viewports where the left panel is gone. */}
          <div className="lg:hidden flex items-center">
            <RustrakWordmark className="h-[22px] w-auto" />
          </div>

          <div className="space-y-3">
            <h1 className="text-3xl font-bold tracking-tight">{headline}</h1>
            <p className="text-muted-foreground leading-relaxed">
              {description}
            </p>
          </div>

          {guidance ? (
            <p className="text-sm text-muted-foreground border-l-2 border-primary/40 pl-4 leading-relaxed">
              {guidance}
            </p>
          ) : null}

          {actions ? (
            <div className="flex flex-wrap gap-3">{actions}</div>
          ) : null}

          {detail ? (
            <p className="text-xs text-muted-foreground/70 font-mono break-all">
              {detail}
            </p>
          ) : null}
        </div>
      </div>
    </div>
  );
}
