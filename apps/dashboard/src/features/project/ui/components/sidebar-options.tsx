import type {
  ProjectResponse,
  SidebarProjects,
  SidebarSort,
} from '@rustrak/client';
import {
  cn,
  DragIcon,
  FilterIcon,
  focusRing,
  interactiveTransition,
  Popover,
  SegmentedControl,
  SegmentedItem,
  Switch,
  Text,
} from '@rustrak/ui';
import { useMutation } from '@tanstack/react-query';
import { PlatformIcon } from 'platformicons';
import { type PointerEvent, useState } from 'react';
import { useTranslations } from 'use-intl';
import { projectMutations } from '@/features/project/api/mutations';
import {
  inManualOrder,
  makeRoom,
  moveTo,
} from '@/features/project/model/sidebar';
import { useToastFailure } from '@/shared/ui/hooks/use-toast-failure';

const SORTS: SidebarSort[] = ['manual', 'name', 'activity'];

/**
 * The options button by the sidebar's "Projects": how the list is sorted,
 * and, project by project, whether it shows and where.
 *
 * Every change is saved the moment it is made, and the sidebar follows at
 * once (the cache takes it before the request). Reordering is by drag,
 * and only the manual sort has an order to change.
 *
 * The drag is pointer events, not HTML drag and drop: Chrome draws the
 * native drag image wrong inside a transformed popup (it picked up the
 * header). The row follows the pointer and the rows it passes slide aside
 * to open its gap; the order only changes on drop, so nothing jumps.
 */
export function SidebarOptions({
  projects,
  prefs,
}: {
  /** Every project, hidden ones included, in the sort `prefs` chose. */
  projects: ProjectResponse[];
  prefs: SidebarProjects;
}) {
  const t = useTranslations('projects');
  const toastFailure = useToastFailure();
  const save = useMutation(projectMutations.saveSidebar());
  const [drag, setDrag] = useState<Drag | null>(null);
  // The order just dropped, shown until its save settles (the cache has it
  // by then), so the list does not flick back while the save is on its way.
  const [dropped, setDropped] = useState<number[] | null>(null);

  // In the sort the sidebar uses; a drop shows at once, before the save.
  const listed =
    prefs.sort === 'manual' && dropped
      ? inManualOrder(projects, dropped)
      : projects;
  const order = listed.map((p) => p.id);
  const hidden = new Set(prefs.hidden);
  const manual = prefs.sort === 'manual';

  const apply = (next: Partial<SidebarProjects>) =>
    save.mutate(
      { ...prefs, ...next },
      { onError: (error) => toastFailure(t('sidebar.saveFailed'), error) },
    );

  // Where the dragged row would land: as many places as the pointer has
  // moved whole rows, within the list.
  const target = drag
    ? Math.max(
        0,
        Math.min(
          order.length - 1,
          drag.from + Math.round(drag.dy / drag.pitch),
        ),
      )
    : 0;

  const grab = (
    event: PointerEvent<HTMLLIElement>,
    id: number,
    from: number,
  ) => {
    // The switch is a control of its own, not a handle.
    const onSwitch = (event.target as Element).closest('[role="switch"]');
    if (!manual || event.button !== 0 || onSwitch) return;
    const row = event.currentTarget;
    row.setPointerCapture(event.pointerId);
    const next = row.nextElementSibling ?? row.previousElementSibling;
    const pitch = next
      ? Math.abs(
          next.getBoundingClientRect().top - row.getBoundingClientRect().top,
        )
      : row.offsetHeight;
    setDrag({ id, from, startY: event.clientY, dy: 0, pitch });
  };

  const follow = (event: PointerEvent<HTMLLIElement>) => {
    if (drag) setDrag({ ...drag, dy: event.clientY - drag.startY });
  };

  const drop = () => {
    if (drag && target !== drag.from) {
      const next = moveTo(order, drag.id, target);
      setDropped(next);
      save.mutate(
        { ...prefs, order: next },
        {
          onError: (error) => toastFailure(t('sidebar.saveFailed'), error),
          onSettled: () => setDropped(null),
        },
      );
    }
    setDrag(null);
  };

  return (
    <Popover
      align="start"
      side="right"
      title={t('sidebar.title')}
      popupClassName="w-80 p-0"
      trigger={
        <button
          type="button"
          aria-label={t('sidebar.options')}
          className={cn(
            'flex size-control-xs items-center justify-center rounded-md',
            'text-fg-meta hover:bg-surface-hover hover:text-fg',
            'data-popup-open:bg-surface-hover data-popup-open:text-fg',
            interactiveTransition,
            focusRing,
          )}
        >
          <FilterIcon size="md" />
        </button>
      }
    >
      <div className="flex flex-col gap-2 border-b border-border-divider px-3 py-3">
        <Text variant="hint" tone="meta">
          {t('sidebar.sortBy')}
        </Text>
        <SegmentedControl
          aria-label={t('sidebar.sortBy')}
          className="w-full"
          value={prefs.sort}
          onValueChange={(sort) => apply({ sort: sort as SidebarSort })}
        >
          {SORTS.map((sort) => (
            <SegmentedItem key={sort} value={sort} className="flex-1">
              {t(`sidebar.sort.${sort}`)}
            </SegmentedItem>
          ))}
        </SegmentedControl>
      </div>

      <div className="flex items-baseline justify-between px-3 pt-3 pb-1">
        <Text variant="hint" tone="meta">
          {t('sidebar.projects')}
        </Text>
        <Text variant="hint" tone="ghost">
          {t('sidebar.shown', {
            shown: listed.filter((p) => !hidden.has(p.id)).length,
            total: listed.length,
          })}
        </Text>
      </div>
      <ul className="relative max-h-80 overflow-y-auto px-1 pb-1">
        {listed.map((project, index) => (
          <ProjectRow
            key={project.id}
            project={project}
            on={!hidden.has(project.id)}
            manual={manual}
            held={drag?.id === project.id}
            dragging={drag !== null}
            offset={rowOffset(drag, project.id, index, target)}
            onPointerDown={(event) => grab(event, project.id, index)}
            onPointerMove={follow}
            onPointerUp={drop}
            onPointerCancel={() => setDrag(null)}
            onToggle={(checked) =>
              apply({
                hidden: checked
                  ? prefs.hidden.filter((id) => id !== project.id)
                  : [...prefs.hidden, project.id],
              })
            }
          />
        ))}
      </ul>
    </Popover>
  );
}

/** The row being dragged: where it started, how far the pointer has moved
 * it, and one row's pitch, which is how far the others slide. */
interface Drag {
  id: number;
  from: number;
  startY: number;
  dy: number;
  pitch: number;
}

/** How far a row is drawn from its place: the held one with the pointer,
 * the ones it passes by a row's pitch, the rest not at all. */
function rowOffset(
  drag: Drag | null,
  id: number,
  index: number,
  target: number,
): number {
  if (!drag) return 0;
  if (drag.id === id) return drag.dy;
  return makeRoom(index, drag.from, target) * drag.pitch;
}

function ProjectRow({
  project,
  on,
  manual,
  held,
  dragging,
  offset,
  onToggle,
  ...pointer
}: {
  project: ProjectResponse;
  on: boolean;
  manual: boolean;
  held: boolean;
  dragging: boolean;
  offset: number;
  onToggle: (checked: boolean) => void;
  onPointerDown: (event: PointerEvent<HTMLLIElement>) => void;
  onPointerMove: (event: PointerEvent<HTMLLIElement>) => void;
  onPointerUp: () => void;
  onPointerCancel: () => void;
}) {
  const t = useTranslations('projects');
  return (
    <li
      {...pointer}
      style={offset ? { translate: `0 ${offset}px` } : undefined}
      className={cn(
        'relative flex items-center gap-2 rounded-md px-2 py-1.5',
        manual && 'cursor-grab touch-none select-none',
        // The rows it passes glide; the held one sticks to the pointer, and
        // nothing animates back once it lands.
        dragging && !held
          ? 'transition-[translate] duration-fast ease-standard motion-reduce:transition-none'
          : null,
        held
          ? 'z-10 cursor-grabbing bg-surface-floating shadow-raised'
          : 'hover:bg-surface-hover',
      )}
    >
      {manual ? (
        <DragIcon
          size="md"
          aria-label={t('sidebar.drag', { name: project.name })}
          className="shrink-0 text-fg-ghost"
        />
      ) : null}
      <PlatformIcon
        platform={project.platform ?? 'other'}
        size={18}
        format="lg"
        className="shrink-0 rounded-md"
      />
      <Text
        variant="value"
        truncate
        className={cn('flex-1', !on && 'text-fg-subtle')}
      >
        {project.name}
      </Text>
      <Switch
        aria-label={t('sidebar.show', { name: project.name })}
        checked={on}
        onCheckedChange={onToggle}
      />
    </li>
  );
}
