import { ArrowRightIcon, CloseIcon, TrendUpIcon } from '@rustrak/ui';
import { useEffect, useId, useLayoutEffect, useRef, useState } from 'react';
import { useTranslations } from 'use-intl';
import type { UpdateInfo } from '@/shared/lib/version';
import { RustrakWordmark } from '@/shared/ui/components/rustrak-wordmark';

const DISMISSED_KEY = 'rustrak:update-dismissed';

const WIDTH = 350;
const HEIGHT = 40;
const ROUNDNESS = 16;
const BLUR = ROUNDNESS * 0.5;
const MIN_EXPANDED = HEIGHT * 2;
const DURATION = 600;
const SHEEN_INSET = 3;

// Spring curve (bounce 0.25, 600ms) sampled as a CSS linear() easing, so the
// blob settles with a real overshoot instead of a cubic-bezier approximation.
const SPRING =
  'linear(0, 0.002 0.6%, 0.007 1.2%, 0.015 1.8%, 0.026 2.4%, 0.041 3.1%, 0.06 3.8%, 0.108 5.3%, 0.157 6.6%, 0.214 8%, 0.467 13.7%, 0.577 16.3%, 0.631 17.7%, 0.682 19.1%, 0.73 20.5%, 0.771 21.8%, 0.808 23.1%, 0.844 24.5%, 0.874 25.8%, 0.903 27.2%, 0.928 28.6%, 0.952 30.1%, 0.972 31.6%, 0.988 33.1%, 1.01 35.7%, 1.025 38.5%, 1.034 41.6%, 1.038 45%, 1.035 50.1%, 1.012 64.2%, 1.003 73%, 0.999 83.7%, 1)';

const KEYFRAMES = `
@keyframes rustrak-banner-in {
  from { opacity: 0; transform: translateY(-18px) scale(0.9); }
  to   { opacity: 1; transform: translateY(0) scale(1); }
}
@keyframes rustrak-banner-glow {
  0%, 100% { opacity: 0.28; transform: scale(1); }
  50%      { opacity: 0.5;  transform: scale(1.06); }
}
@keyframes rustrak-banner-sheen {
  0%        { transform: translateX(-140%) skewX(-20deg); }
  55%, 100% { transform: translateX(420%)  skewX(-20deg); }
}
@media (prefers-reduced-motion: reduce) {
  [data-rustrak-idle], [data-rustrak-enter] { animation: none !important; }
  /* The expansion is driven by transitions, not keyframes, so killing the
     animations alone still leaves the spring on hover and focus. Every value
     here is computed from state, so with no transition they simply snap. */
  [data-rustrak-enter], [data-rustrak-enter] * { transition: none !important; }
}
`;

interface UpdateBannerProps {
  info: UpdateInfo;
}

export function UpdateBanner({ info }: UpdateBannerProps) {
  const t = useTranslations('update');
  const [dismissed, setDismissed] = useState<string | null>(null);
  const [mounted, setMounted] = useState(false);
  const [open, setOpen] = useState(false);
  const [pillWidth, setPillWidth] = useState(0);
  const [contentHeight, setContentHeight] = useState(0);

  // useId() yields ids containing ':', which is invalid inside a CSS url(#id).
  const filterId = `goo${useId().replace(/[^a-zA-Z0-9]/g, '')}`;
  const headerRef = useRef<HTMLDivElement>(null);
  const bodyRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    try {
      setDismissed(localStorage.getItem(DISMISSED_KEY));
    } catch {
      // Storage can be unavailable in private mode; treat as not dismissed.
    }
    setMounted(true);
  }, []);

  const visible = mounted && dismissed !== info.latest;

  // Depends on `visible`: the first render returns null, so the refs are only
  // attached on the render where this becomes true.
  useLayoutEffect(() => {
    if (!visible) return;
    const header = headerRef.current;
    const body = bodyRef.current;
    if (!header || !body) return;

    const measure = () => {
      setPillWidth(header.scrollWidth);
      setContentHeight(body.scrollHeight);
    };
    measure();

    const observer = new ResizeObserver(measure);
    observer.observe(header);
    observer.observe(body);
    return () => observer.disconnect();
  }, [visible]);

  if (!visible) return null;

  const handleDismiss = () => {
    try {
      localStorage.setItem(DISMISSED_KEY, info.latest);
    } catch {
      // Dismissal is best-effort; hiding it for this session is enough.
    }
    setDismissed(info.latest);
  };

  const ready = pillWidth > 0;
  const pill = Math.max(pillWidth, HEIGHT);
  const pillX = (WIDTH - pill) / 2;
  const expanded = Math.max(MIN_EXPANDED, HEIGHT + contentHeight);
  const ease = ready ? `${DURATION}ms ${SPRING}` : '0s';

  // Each row lifts into place slightly after the blob has formed.
  const reveal = (index: number) => ({
    opacity: open ? 1 : 0,
    transform: open ? 'translateY(0)' : 'translateY(8px)',
    transition: `opacity 240ms ease-out, transform 320ms ${SPRING}`,
    transitionDelay: open ? `${170 + index * 55}ms` : '0ms',
  });

  return (
    <div className="pointer-events-none fixed top-3 left-1/2 z-50 -translate-x-1/2">
      {/* The pill only reads as an update to sighted users; without this a
          screen reader user learns nothing until they tab into it. */}
      <span role="status" className="sr-only">
        {t('srAvailable', { version: info.latest })}
      </span>

      {/* biome-ignore lint/security/noDangerouslySetInnerHtml: static keyframes, no user input */}
      <style dangerouslySetInnerHTML={{ __html: KEYFRAMES }} />

      <svg width="0" height="0" className="absolute" aria-hidden="true">
        <title>{t('shapeTitle')}</title>
        <defs>
          <filter id={filterId} colorInterpolationFilters="sRGB">
            <feGaussianBlur
              in="SourceGraphic"
              stdDeviation={BLUR}
              result="blur"
            />
            <feColorMatrix
              in="blur"
              mode="matrix"
              values="1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 20 -10"
              result="goo"
            />
            <feComposite in="SourceGraphic" in2="goo" operator="atop" />
          </filter>
        </defs>
      </svg>

      <div
        data-rustrak-enter
        style={{
          width: WIDTH,
          height: expanded,
          opacity: ready ? undefined : 0,
          animation: ready
            ? `rustrak-banner-in 700ms ${SPRING} both`
            : undefined,
        }}
        className="relative"
        onMouseEnter={() => setOpen(true)}
        onMouseLeave={() => setOpen(false)}
        onFocus={() => setOpen(true)}
        onBlur={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget))
            setOpen(false);
        }}
      >
        <div
          aria-hidden="true"
          data-rustrak-idle
          className="absolute bg-surface-brand"
          style={{
            left: pillX,
            top: 0,
            width: pill,
            height: HEIGHT,
            borderRadius: ROUNDNESS,
            filter: 'blur(18px)',
            animation: 'rustrak-banner-glow 4s ease-in-out infinite',
          }}
        />

        <div
          aria-hidden="true"
          className="absolute inset-0"
          style={{
            filter: `url(#${filterId})`,
            // Promotes the filtered layer so the browser does not re-rasterize
            // at the end of the height transition, which caused a 1px jump.
            transform: 'translateZ(0)',
            contain: 'layout style',
          }}
        >
          <div
            className="absolute top-0 bg-surface-brand"
            style={{
              left: pillX,
              width: pill,
              height: open ? HEIGHT + BLUR * 3 : HEIGHT,
              borderRadius: ROUNDNESS,
              // Only while the pill is actually moving. A permanent
              // will-change keeps a compositor layer alive for the whole
              // session for an animation that runs twice.
              willChange: open ? 'height, width' : 'auto',
              transition: `left ${ease}, width ${ease}, height ${ease}`,
            }}
          />
          <div
            className="absolute left-0 w-full bg-surface-brand"
            style={{
              top: HEIGHT,
              height: open ? expanded - HEIGHT : 0,
              opacity: open ? 1 : 0,
              borderRadius: ROUNDNESS,
              willChange: open ? 'height' : 'auto',
              transition: `height ${ease}, opacity 200ms ease-out`,
            }}
          />
        </div>

        {/* Inset by SHEEN_INSET: the gooey filter rounds the pill past its own
            border-radius, so an exact-size mask spills outside the solid fill. */}
        <div
          aria-hidden="true"
          className="absolute overflow-hidden"
          style={{
            left: pillX + SHEEN_INSET,
            top: SHEEN_INSET,
            width: pill - SHEEN_INSET * 2,
            height: HEIGHT - SHEEN_INSET * 2,
            borderRadius: ROUNDNESS - SHEEN_INSET,
            // Faded out rather than paused: pausing froze the glint mid-sweep.
            opacity: open ? 0 : 1,
            transition: 'opacity 200ms ease-out',
          }}
        >
          <div
            data-rustrak-idle
            className="h-full w-1/3 bg-linear-to-r from-transparent via-white/40 to-transparent"
            style={{
              animation: 'rustrak-banner-sheen 5s ease-in-out infinite',
            }}
          />
        </div>

        <div
          ref={headerRef}
          style={{ left: pillX, height: HEIGHT, width: 'fit-content' }}
          className="pointer-events-auto absolute top-0 z-20 flex items-center gap-2 pr-3 pl-4 text-fg-on-brand"
        >
          {/* 14px is the brand's absolute floor and the pill is the one place
              in the app that reaches it: any taller and the word crowds out the
              version it is announcing. `still` because the pill is already lime:
              a mark that tints to lime on hover would tint to invisible. */}
          <RustrakWordmark className="h-3.5 w-auto shrink-0" still />
          <span className="text-xs whitespace-nowrap">
            <span className="font-semibold">{info.latest}</span>{' '}
            {t('available')}
          </span>
          <button
            type="button"
            onClick={handleDismiss}
            aria-label={t('dismiss', { version: info.latest })}
            className="inline-flex size-5 shrink-0 items-center justify-center rounded-full transition-colors hover:bg-fg-on-brand/20"
          >
            <CloseIcon className="size-3" />
          </button>
        </div>

        <div
          ref={bodyRef}
          style={{ top: HEIGHT + 2 }}
          className={`absolute inset-x-0 z-20 flex flex-col items-center px-6 pt-1.5 pb-4 text-center text-fg-on-brand ${
            open ? 'pointer-events-auto' : 'pointer-events-none'
          }`}
        >
          <div
            style={reveal(0)}
            className="flex items-center gap-2.5 font-mono text-xs tabular-nums"
          >
            <span className="opacity-55">{info.current}</span>
            <ArrowRightIcon className="size-3.5 opacity-45" />
            <span className="text-sm font-semibold">{info.latest}</span>
          </div>

          {info.description && (
            <p
              style={reveal(1)}
              className="mt-2.5 line-clamp-2 text-[11px] leading-relaxed text-balance opacity-80"
            >
              {info.description}
            </p>
          )}

          <div
            style={reveal(2)}
            className="mt-3 w-full border-t border-fg-on-brand/20 pt-3"
          >
            <a
              href={info.url}
              target="_blank"
              rel="noopener noreferrer"
              className="group inline-flex items-center gap-1.5 rounded-full bg-fg-on-brand px-4 py-1.5 text-[11px] font-semibold text-fg-brand transition-opacity hover:opacity-90"
            >
              {t('viewChangelog')}
              <TrendUpIcon className="size-3 transition-transform group-hover:translate-x-0.5 group-hover:-translate-y-0.5" />
            </a>{' '}
          </div>
        </div>
      </div>
    </div>
  );
}
