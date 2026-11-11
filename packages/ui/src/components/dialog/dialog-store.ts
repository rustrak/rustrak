import { type ComponentType, createContext } from 'react';
import type { DialogSize } from './dialog';

/**
 * The dismiss cross's accessible name, in the reader's language. The app
 * hands it to `DialogProvider`; English only stands in without one.
 */
export const DialogCloseLabel = createContext('Close');

/**
 * How modals open.
 *
 * In a dashboard a modal is almost never "a part of the screen that
 * sometimes shows": it is a question asked in the middle of a function --
 * pick a project, confirm this -- whose answer is needed on the next line.
 * Written as screen state, that function splits into three pieces and the
 * modal rides the tree of a screen that is not about it.
 *
 * So they open by being called, and they are awaited:
 *
 * ```tsx
 * const project = await pickProject.open({ team });
 * if (!project) return;                // closed without choosing
 * assign(issue, project);
 * ```
 *
 * What renders does **not** leave the React tree: a `DialogProvider` sits at
 * the root and is what mounts the modal, so the theme, the data client and
 * everything else that travels by context still hold inside. The registry
 * lives in a module rather than a context so `open()` can also be called
 * from where there is no component: a table handler, a domain function, a
 * request interceptor.
 */

export interface DialogHostProps<Result> {
  /**
   * Closes the modal and hands the answer to whoever opened it. With no
   * argument -- or closing with Escape, the cross or a click outside -- the
   * answer is `undefined`, which is how "declined" is told apart from
   * "chose this".
   */
  close: (result?: Result) => void;
}

export type DialogComponent<Props, Result> = ComponentType<
  Props & DialogHostProps<Result>
>;

export interface DialogDefinitionOptions {
  size?: DialogSize;
  /** Does not close on an outside click. For what must be answered. */
  alert?: boolean;
}

export interface DialogDefinition<Props, Result> {
  /** Opens the modal and waits for the answer. */
  open: OpenFunction<Props, Result>;
  /** Closes this modal's open instance, if there is one. */
  close: () => void;
}

/** With no required props, `open()` is callable with no arguments. */
type OpenFunction<Props, Result> =
  Record<string, never> extends Props
    ? (props?: Props) => Promise<Result | undefined>
    : (props: Props) => Promise<Result | undefined>;

/** The registry holds the modals without their types: `createDialog` is who
 *  takes them back out, and it does know them. */
type AnyDialogComponent = ComponentType<
  Record<string, unknown> & DialogHostProps<unknown>
>;

export interface Entry {
  id: number;
  Component: AnyDialogComponent;
  props: Record<string, unknown>;
  options: DialogDefinitionOptions;
  open: boolean;
  settle: (result: unknown) => void;
}

let nextId = 0;
let entries: Entry[] = [];
let mounted = 0;
const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) {
    listener();
  }
}

export function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function getSnapshot() {
  return entries;
}

/** A server opens no modals: there, the stack is always empty. */
const EMPTY: Entry[] = [];
export function getServerSnapshot() {
  return EMPTY;
}

function push(entry: Entry) {
  entries = [...entries, entry];
  emit();
}

/**
 * Closing happens in two beats: first the entry is marked closed, which is
 * what runs the exit animation, and only when that finishes is it removed
 * from the list. Removing it outright would vanish the modal between one
 * frame and the next.
 */
export function close(id: number, result: unknown) {
  const entry = entries.find((item) => item.id === id);
  if (!entry?.open) {
    return;
  }

  entries = entries.map((item) =>
    item.id === id ? { ...item, open: false } : item,
  );
  emit();
  entry.settle(result);
}

export function remove(id: number) {
  entries = entries.filter((item) => item.id !== id);
  emit();
}

/** A `<DialogProvider>` mounting or unmounting, so `createDialog`'s `open()`
 *  can warn in development when none is mounted. */
export function registerHost() {
  mounted += 1;
  return () => {
    mounted -= 1;
  };
}

/**
 * Defines a modal. Done once, beside the component that draws it, and from
 * then on it opens from wherever it is needed.
 *
 * ```tsx
 * export const pickProject = createDialog<{ team: string }, Project>(
 *   function PickProject({ team, close }) {
 *     return (
 *       <>
 *         <DialogHeader title="Choose a project" />
 *         <DialogBody>…</DialogBody>
 *         <DialogFooter>
 *           <Button onClick={() => close(project)}>Use</Button>
 *         </DialogFooter>
 *       </>
 *     );
 *   },
 *   { size: 'xl' },
 * );
 * ```
 *
 * The component mounts only while the modal is open, so it can fetch its
 * data, hold its state and use whatever hooks it needs without any of it
 * weighing on the screen that opened it.
 */
export function createDialog<Props = Record<string, never>, Result = void>(
  Component: DialogComponent<Props, Result>,
  options: DialogDefinitionOptions = {},
): DialogDefinition<Props, Result> {
  let openId: number | null = null;

  const open = (props?: Props) =>
    new Promise<Result | undefined>((resolve) => {
      if (process.env.NODE_ENV !== 'production' && mounted === 0) {
        console.error(
          '[@rustrak/ui] A dialog was opened with no <DialogProvider> mounted: nothing will show.',
        );
      }

      const id = nextId++;
      openId = id;

      push({
        id,
        Component: Component as unknown as AnyDialogComponent,
        props: (props ?? {}) as Record<string, unknown>,
        options,
        open: true,
        settle: (result) => {
          if (openId === id) {
            openId = null;
          }
          resolve(result as Result | undefined);
        },
      });
    });

  return {
    open: open as OpenFunction<Props, Result>,
    close: () => {
      if (openId != null) {
        close(openId, undefined);
      }
    },
  };
}
