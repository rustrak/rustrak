'use client';

import { Check, Copy } from 'lucide-react';
import { useState } from 'react';
import { Band } from '@/components/frame/grid';
import { Heading, Pill } from '../primitives/heading';

/** Verbatim from content/getting-started/installation.mdx. */
const VARIANTS = {
  SQLite: `docker run -d \\
  --name rustrak \\
  -p 8080:8080 \\
  -v rustrak_data:/data \\
  -e SESSION_SECRET_KEY=$(openssl rand -hex 32) \\
  -e CREATE_SUPERUSER=admin@example.com:changeme123 \\
  rustrak/rustrak-server:latest`,
  PostgreSQL: `docker run -d \\
  --name rustrak \\
  -p 8080:8080 \\
  -e DATABASE_URL="postgres://user:pass@db-host:5432/rustrak" \\
  -e SESSION_SECRET_KEY="$(openssl rand -hex 32)" \\
  -e CREATE_SUPERUSER="admin@example.com:changeme123" \\
  rustrak/rustrak-server:postgres`,
} as const;

type Variant = keyof typeof VARIANTS;

/**
 * Colours the command so it reads like a terminal rather than a grey block:
 * the binary, its flags and the image tag each in their own tone.
 */
function highlight(line: string, index: number) {
  if (index === 0) {
    const [, binary, rest] = line.match(/^(\S+\s+\S+\s+\S+)(.*)$/) ?? [];
    return (
      <>
        <span className="text-primary">{binary ?? line}</span>
        <span className="text-white/55">{rest}</span>
      </>
    );
  }

  const flag = line.match(/^(\s*)(-{1,2}[a-z]+)(.*)$/i);
  if (flag) {
    return (
      <>
        {flag[1]}
        <span className="text-sky-300/80">{flag[2]}</span>
        <span className="text-white/55">{flag[3]}</span>
      </>
    );
  }

  return <span className="text-white/80">{line}</span>;
}

export function OneCommand() {
  const [variant, setVariant] = useState<Variant>('SQLite');
  const [copied, setCopied] = useState(false);

  /*
    `navigator.clipboard` is not there at all outside a secure context, and
    `writeText` rejects on a denied permission. Unguarded, the click threw an
    unhandled rejection and the button sat unchanged, which reads to the reader
    as a button that does nothing rather than as a copy that failed — so the
    command is left selectable and the state is simply not claimed.
  */
  const copy = async () => {
    try {
      /* Not `navigator.clipboard?.`: optional chaining on a missing clipboard
         resolves rather than throwing, and the button would then claim a copy
         that never happened. Reaching straight through means the absent case
         and the rejected case arrive at the same place. */
      await navigator.clipboard.writeText(VARIANTS[variant]);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 2000);
    } catch {
      setCopied(false);
    }
  };

  const command = VARIANTS[variant];

  return (
    <Band>
      <div className="px-4 py-14 text-center sm:px-10 sm:py-24">
        <Pill>Install</Pill>
        <Heading
          className="display-lg mx-auto mt-6 max-w-[22ch]"
          lead="Up with one command."
          rest="Docker, SQLite by default, and an admin user created on first boot."
          scrub
        />

        <div className="mx-auto mt-9 max-w-[720px] text-left sm:mt-12">
          <div className="overflow-hidden rounded-xl border border-white/12 bg-[oklch(0.115_0_0)]">
            <div className="flex items-center gap-1 border-b border-white/8 px-3 py-2">
              {(Object.keys(VARIANTS) as Variant[]).map((name) => (
                <button
                  key={name}
                  type="button"
                  onClick={() => setVariant(name)}
                  className={`rounded-md px-2.5 py-2 text-[12px] transition-colors sm:py-1 ${
                    variant === name
                      ? 'bg-white/10 text-white'
                      : 'text-white/40 hover:text-white/70'
                  }`}
                >
                  {name}
                </button>
              ))}

              <button
                type="button"
                onClick={copy}
                className="ml-auto inline-flex items-center gap-1.5 rounded-md px-2 py-2 text-[12px] text-white/45 transition-colors hover:text-white sm:py-1"
              >
                {copied ? (
                  <Check className="size-3.5 text-primary" />
                ) : (
                  <Copy className="size-3.5" />
                )}
                {copied ? 'Copied' : 'Copy'}
              </button>
            </div>

            {/*
              The command keeps its line breaks and scrolls sideways rather
              than wrapping. A wrapped shell command is a different command as
              far as the eye is concerned — the `\` continuations stop lining
              up and a flag can end up looking like an argument to the one
              above it. `overscroll-x-contain` keeps a swipe that reaches the
              end of the block from turning into a page-level back gesture.

              Below `sm` the two longest flags run past the edge, so the block
              gets a fade there to say it can be dragged. `sm:hidden` rather
              than measured: above that width the command fits, and a scroll
              hint over content that does not scroll is worse than none.
            */}
            <div className="relative">
              <pre className="overflow-x-auto overscroll-x-contain px-4 py-4 font-mono text-[11.5px] leading-[1.85] sm:px-5 sm:py-5 sm:text-[12.5px]">
                <code>
                  {command.split('\n').map((line, index) => (
                    // The command is a fixed authored string, never reordered.
                    // react-doctor-disable-next-line react-doctor/no-array-index-as-key
                    <span key={index} className="block whitespace-pre">
                      {highlight(line, index)}
                    </span>
                  ))}
                </code>
              </pre>

              <div
                aria-hidden
                className="pointer-events-none absolute inset-y-0 right-0 w-14 bg-gradient-to-l from-[oklch(0.115_0_0)] via-[oklch(0.115_0_0)]/70 to-transparent sm:hidden"
              />
            </div>
          </div>

          <p className="mt-4 text-center text-[13px] text-muted-foreground">
            Open <span className="font-mono text-white/70">localhost:8080</span>
            : the dashboard and the API, on one port. SQLite by default, so
            there is no second service to stand up.
          </p>
        </div>
      </div>
    </Band>
  );
}
