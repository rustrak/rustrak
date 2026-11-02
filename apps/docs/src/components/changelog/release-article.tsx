import { Tag, type TagTone } from '@rustrak/ui';
import {
  formatReleaseDate,
  type Release,
  type SectionKind,
} from '@/lib/release';
import { cn } from '@/lib/utils';
import { Reveal } from '../home/reveal';

const KIND_TONE: Record<SectionKind, TagTone> = {
  shipped: 'brand',
  improved: 'neutral',
  fixed: 'warning',
  documented: 'info',
};

/**
 * One release, laid out like a landing feature: the version held on the left
 * while its notes scroll past in a card on the right. A patch is drawn a step
 * quieter than a minor, so the shape of the history shows while scrolling.
 */
export function ReleaseArticle({ release }: { release: Release }) {
  const minor = release.channel === 'minor';

  return (
    <article
      id={release.anchor}
      className="grid scroll-mt-24 gap-8 border-border-subtle border-t py-16 lg:grid-cols-[minmax(0,1fr)_minmax(0,2.4fr)] lg:gap-16"
    >
      {/* The filename, kept as a second anchor so old deep links still land. */}
      <span id={release.slug} aria-hidden className="hidden" />

      <header className="flex flex-col gap-3 lg:sticky lg:top-28 lg:self-start">
        <Reveal y={16}>
          <a
            href={`#${release.anchor}`}
            className={cn(
              'text-headline tabular-nums transition-colors hover:text-fg-brand',
              minor ? 'text-fg' : 'text-fg-subtle',
            )}
          >
            {release.version}
          </a>
        </Reveal>
        <Reveal delay={0.1} className="flex flex-wrap items-center gap-3">
          <time dateTime={release.date} className="text-fg-meta text-meta">
            {formatReleaseDate(release.date)}
          </time>
          <Tag variant="soft" tone={minor ? 'brand' : 'neutral'}>
            {release.channel === 'seed' ? 'early' : release.channel}
          </Tag>
        </Reveal>
        {release.tags.length > 0 ? (
          <Reveal delay={0.15}>
            <p className="font-mono text-fg-meta text-mono-sm">
              {release.tags.join(' · ')}
            </p>
          </Reveal>
        ) : null}
      </header>

      <Reveal
        y={40}
        delay={0.1}
        className="min-w-0 rounded-3xl border border-border-subtle bg-panel p-6 sm:p-8"
      >
        <h2 className="text-balance text-fg text-page-title">
          {release.title}
        </h2>
        {release.description ? (
          <p className="changelog-lead mt-3">{release.description}</p>
        ) : null}

        {release.sections.map((section, index) => (
          <section
            // Sections of a fixed release never reorder; an untitled one has only its position.
            // react-doctor-disable-next-line react-doctor/no-array-index-as-key
            key={section.title || index}
            className="mt-8 border-border-subtle border-t pt-6 first:mt-8"
          >
            {section.title ? (
              <h3 className="mb-4">
                <Tag tone={KIND_TONE[section.kind]}>{section.title}</Tag>
              </h3>
            ) : null}
            <div
              className="changelog-prose max-w-[64ch]"
              // react-doctor-disable-next-line react-doctor/dangerous-html-sink
              // biome-ignore lint/security/noDangerouslySetInnerHtml: the only input is content/changelog/*.mdx, rendered by lib/changelog.ts at build time — nothing here is reachable from a request
              dangerouslySetInnerHTML={{ __html: section.html }}
            />
          </section>
        ))}
      </Reveal>
    </article>
  );
}
