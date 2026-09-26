import { PlatformIcon } from 'platformicons';
import { type Ref, useMemo } from 'react';
import { useTranslations } from 'use-intl';
import {
  ALL_PROJECT_PAGES,
  type CommandLink,
  type CommandProject,
  PROJECT_COMMANDS,
  PROJECT_PAGE_COUNT,
  type ProjectPage,
  SETTINGS_COMMANDS,
} from '@/shared/config/commands';
import { filterCommand } from '@/shared/lib/command-score';
import {
  CommandEmpty,
  CommandList,
} from '@/shared/ui/components/shadcn/command';
import { Kbd } from '@/shared/ui/components/shadcn/kbd';
import { Section } from '../primitives/section';
import { ProjectBadge, ScopeBadge } from './badges';
import { CommandRow } from './command-row';
import type { CommandTarget } from './command-target';

/**
 * How many project-page rows a search will render.
 *
 * The flattened set is every project times every page, which on a full
 * instance is a thousand rows that cmdk would mount and then rescore on each
 * keystroke. So the matching runs here first, in plain JS over strings, and
 * only the best rows are ever mounted. Nothing is lost that anyone would have
 * read: a query returning more than this many project pages is a query that
 * needed narrowing, and the static links below are never subject to the cap.
 */
const SEARCH_ROW_LIMIT = 50;

export function ResultsList({
  ref,
  projects,
  query,
  onNavigate,
  onHighlight,
}: {
  /**
   * The scroll container. Held by the palette so it can put the list back at
   * the top on each keystroke -- see `CommandBarDialog`.
   */
  ref: Ref<HTMLDivElement>;
  projects: CommandProject[];
  query: string;
  onNavigate: (target: CommandTarget) => void;
  /** Moves cmdk's selection to a hovered row, when the palette wants it moved. */
  onHighlight: (value: string) => void;
}) {
  const t = useTranslations();
  // Collapsed at rest, flattened while searching. A project is one row until
  // the viewer types, at which point every project's pages become candidates
  // and the list only gets long once a query is there to make it short again.
  const searching = query.trim().length > 0;

  /**
   * The project pages a query matches, best first and capped.
   *
   * Scored here rather than handed to cmdk row by row. It is the same function
   * cmdk would have called through `filter`, over the same string, so the rows
   * that survive are exactly the rows it would have kept and it still does the
   * final ordering -- the difference is that the thousand that do not survive
   * were never mounted. See `SEARCH_ROW_LIMIT`.
   */
  const matches = useMemo(() => {
    if (query.trim().length === 0) return [];

    const scored: {
      project: CommandProject;
      page: ProjectPage;
      value: string;
      score: number;
    }[] = [];

    for (const project of projects) {
      for (const page of ALL_PROJECT_PAGES) {
        const value = `${project.name} ${t(page.labelKey)}`;
        const score = filterCommand(value, query, page.keywords);
        if (score > 0) scored.push({ project, page, value, score });
      }
    }

    // Stable, so projects that score alike stay in the order they arrived.
    return scored.sort((a, b) => b.score - a.score).slice(0, SEARCH_ROW_LIMIT);
  }, [projects, query, t]);

  // Only ever rendered by the search, where every project's pages sit in one
  // list, so the badge is unconditional: without it "Issues" appears once per
  // project with nothing to tell the rows apart.
  const pageRow = (
    project: CommandProject,
    page: ProjectPage,
    value: string,
  ) => (
    <CommandRow
      key={`${project.id}${page.to}`}
      value={value}
      keywords={page.keywords}
      label={t(page.labelKey)}
      description={t(page.descriptionKey)}
      badge={<ProjectBadge project={project} />}
      media={<page.icon className="size-[18px] text-muted-foreground" />}
      onSelect={() => onNavigate({ to: page.to, params: { id: project.id } })}
      onHover={() => onHighlight(value)}
    />
  );

  const linkRows = (links: CommandLink[], badgeLabel?: string) =>
    links.map((link) => (
      <CommandRow
        key={link.to}
        value={t(link.labelKey)}
        keywords={link.keywords}
        label={t(link.labelKey)}
        description={t(link.descriptionKey)}
        badge={badgeLabel ? <ScopeBadge label={badgeLabel} /> : undefined}
        media={<link.icon className="size-[18px] text-muted-foreground" />}
        onSelect={() => onNavigate({ to: link.to })}
        onHover={() => onHighlight(t(link.labelKey))}
      />
    ));

  return (
    <CommandList ref={ref} className="max-h-none min-w-0 flex-1 p-2">
      <CommandEmpty className="px-4 py-16 text-center text-sm text-muted-foreground">
        {t('commands.noResultsFor', { query: query.trim() })}
      </CommandEmpty>

      {searching ? (
        // One flat, relevance-ordered list. Grouping by project here spends a
        // heading on every match, which for a query that hits one page per
        // project is three headings for three rows.
        <Section heading={t('commands.resultsHeading')}>
          {matches.map(({ project, page, value }) =>
            pageRow(project, page, value),
          )}
          {linkRows(PROJECT_COMMANDS, t('commands.generalBadge'))}
          {linkRows(SETTINGS_COMMANDS, t('commands.settingsBadge'))}
        </Section>
      ) : (
        <>
          <Section heading={t('commands.projectsHeading')}>
            {projects.map((project) => (
              <CommandRow
                key={project.id}
                value={project.name}
                keywords={['project', project.platform ?? '']}
                label={project.name}
                description={`${project.platform ?? t('commands.noPlatform')} · ${PROJECT_PAGE_COUNT} ${t('commands.pages')}`}
                media={
                  <PlatformIcon
                    platform={project.platform ?? 'other'}
                    size={20}
                    format="lg"
                    className="rounded-[3px]"
                  />
                }
                // Enter goes to the project, like every other row goes to its
                // page. Its pages are one Tab away instead of behind a mode
                // that replaced the whole list.
                onSelect={() =>
                  onNavigate({
                    to: '/projects/$id',
                    params: { id: project.id },
                  })
                }
                onHover={() => onHighlight(project.name)}
                // Advertises the shortcut on the row it applies to, which is
                // the only place anyone would look for it.
                trailing={
                  <Kbd className="opacity-0 transition-opacity duration-100 group-data-[selected=true]/command-item:opacity-100">
                    ⇥
                  </Kbd>
                }
              />
            ))}
            {linkRows(PROJECT_COMMANDS)}
          </Section>

          <Section heading={t('commands.settingsHeading')}>
            {linkRows(SETTINGS_COMMANDS)}
          </Section>
        </>
      )}
    </CommandList>
  );
}
