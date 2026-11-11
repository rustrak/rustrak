import { Wordmark } from '@rustrak/ui';
import { PlatformIcon } from 'platformicons';
import { type CSSProperties, useEffect, useRef } from 'react';
import { runStrikes } from './login-strikes';

const RINGS = [
  {
    radius: '24rem',
    duration: '180s',
    platforms: [
      'javascript-react',
      'python-django',
      'android',
      'php-laravel',
      'flutter',
      'ruby-rails',
      'javascript-vue',
      'elixir',
    ],
  },
  {
    radius: '17rem',
    duration: '130s',
    platforms: ['node', 'java', 'php', 'ruby', 'dotnet', 'apple-ios'],
  },
  {
    radius: '10rem',
    duration: '90s',
    platforms: ['javascript', 'python', 'rust', 'go'],
  },
];

/**
 * The SDKs that report to Rustrak, circling it and striking it with light
 * (`login-strikes.ts`). Each ring turns as one, and each tile turns back at the
 * same speed so its logo stays upright.
 */
export function LoginOrbit() {
  const stage = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const stageEl = stage.current;
    const canvasEl = canvas.current;
    if (!stageEl || !canvasEl) return;

    // The effect reads the theme's colours when it starts, so a new theme, like
    // a new motion preference, means starting it over.
    const motion = matchMedia('(prefers-reduced-motion: reduce)');
    let stop: (() => void) | undefined;
    const restart = () => {
      stop?.();
      stop = motion.matches ? undefined : runStrikes(canvasEl, stageEl);
    };
    restart();

    motion.addEventListener('change', restart);
    const theme = new MutationObserver(restart);
    theme.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme'],
    });

    return () => {
      stop?.();
      motion.removeEventListener('change', restart);
      theme.disconnect();
    };
  }, []);

  return (
    <>
      <div
        ref={stage}
        aria-hidden="true"
        className="pointer-events-none absolute top-[44%] left-1/2 size-[64rem] -translate-1/2 motion-safe:animate-login-fade"
      >
        <div
          data-orbit-mark
          className="absolute top-1/2 left-1/2 -translate-1/2 text-fg"
        >
          <Wordmark still className="block h-[26px] w-auto" />
        </div>

        {RINGS.map(({ radius, duration, platforms }) => (
          <div
            key={radius}
            style={
              {
                '--orbit-duration': duration,
                width: `calc(2 * ${radius})`,
                height: `calc(2 * ${radius})`,
              } as CSSProperties
            }
            className="absolute inset-0 m-auto rounded-full border border-border motion-safe:animate-login-orbit"
          >
            {platforms.map((platform, index) => {
              const angle = `${(360 / platforms.length) * index}deg`;
              return (
                <div
                  key={platform}
                  className="absolute top-1/2 left-1/2"
                  style={{
                    transform: `translate(-50%, -50%) rotate(${angle}) translateY(calc(-1 * ${radius})) rotate(calc(-1 * ${angle}))`,
                  }}
                >
                  <div
                    data-orbit-tile
                    className="flex size-11 items-center justify-center rounded-xl border border-border-subtle bg-surface motion-safe:animate-login-orbit motion-safe:[animation-direction:reverse]"
                  >
                    <PlatformIcon
                      platform={platform}
                      size={24}
                      className="rounded-sm"
                    />
                  </div>
                </div>
              );
            })}
          </div>
        ))}
      </div>
      <canvas
        ref={canvas}
        className="pointer-events-none absolute inset-0 size-full"
      />
    </>
  );
}
