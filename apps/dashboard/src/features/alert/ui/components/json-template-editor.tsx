import {
  autocompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
  closeBrackets,
  closeBracketsKeymap,
  completionKeymap,
} from '@codemirror/autocomplete';
import {
  defaultKeymap,
  history,
  historyKeymap,
  indentWithTab,
} from '@codemirror/commands';
import { json } from '@codemirror/lang-json';
import {
  bracketMatching,
  HighlightStyle,
  indentOnInput,
  indentUnit,
  syntaxHighlighting,
} from '@codemirror/language';
import { lintGutter, setDiagnostics } from '@codemirror/lint';
import { Compartment, EditorState } from '@codemirror/state';
import {
  placeholder as cmPlaceholder,
  Decoration,
  type DecorationSet,
  drawSelection,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  keymap,
  lineNumbers,
  tooltips,
  ViewPlugin,
  type ViewUpdate,
  WidgetType,
} from '@codemirror/view';
import { tags } from '@lezer/highlight';
import {
  AutofixIcon,
  Button,
  ColumnsIcon,
  HelpIcon,
  Menu,
  MenuActions,
  MenuGroup,
  MenuGroupLabel,
  NewIcon,
  PayloadIcon,
} from '@rustrak/ui';
import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslations } from 'use-intl';
import { expressionAt } from '@/features/alert/lib/template-completion';
import { locateTemplateError } from '@/features/alert/lib/template-diagnostic';
import { findExpressions } from '@/features/alert/lib/template-pills';
import {
  isKnownVariable,
  TEMPLATE_VARIABLE_GROUPS,
  type TemplatePreset,
  type TemplateVariable,
  type TemplateVariableGroup,
} from '@/features/alert/model/message-template';
import type { TemplatePreview } from '@/features/alert/ui/hooks/use-template-preview';
import { cn } from '@/shared/lib/utils';

/**
 * The message body is a template that renders to JSON, which is two languages
 * in one field: JSON everywhere, and a field reference between `{{` and `}}`.
 * A plain textarea leaves the reader to hold both in their head, and the only
 * feedback they get is a save that fails.
 *
 * CodeMirror rather than Monaco: this is one small field, not an IDE, and the
 * difference is roughly 250 KB against 2-5 MB.
 *
 * What the editor adds, in order of how much it helps:
 *
 * 1. Fields as pills. `{{ issue.title }}` is drawn as one object, the way
 *    Zapier and Notion draw an inserted variable: deleted in one keystroke,
 *    never edited from inside, and coloured red when it names a field the
 *    payload does not have. Expressions with operators or filters stay text,
 *    because somebody wrote those on purpose.
 * 2. A place to start. The "Start from" menu drops in the shape a known
 *    service expects; the "Insert field" menu drops in a pill at the caret.
 *    Typing `{{` offers the same list inline.
 * 3. The rendered body underneath, from the server's own renderer, a moment
 *    after the reader stops typing. A refusal is underlined where it breaks.
 *
 * The CodeMirror packages are imported statically here on purpose, and
 * `react-doctor/prefer-dynamic-import` is turned off for this file in
 * `doctor.config.json` because of it: this whole component is already behind a
 * `lazy()` boundary in the form that uses it, so it is never in the page's
 * bundle. Splitting the imports again inside it would buy nothing and cost a
 * second waterfall.
 */
export function JsonTemplateEditor({
  value,
  onChange,
  onBlur,
  onFormat,
  onPreset,
  preview,
  disabled,
  placeholder,
  variables,
  presets,
  ariaLabel,
  helpHref,
}: {
  value: string;
  onChange: (value: string) => void;
  onBlur?: () => void;
  /** Rewrites the body indented. Wired to the wand in the header. */
  onFormat: () => void;
  /** Replaces the body with a preset. The form owns the undo toast. */
  onPreset: (preset: TemplatePreset) => void;
  /** What the server says the body renders to; the form asks, this shows. */
  preview: TemplatePreview;
  disabled: boolean;
  placeholder: string;
  variables: readonly TemplateVariable[];
  presets: readonly TemplatePreset[];
  ariaLabel: string;
  helpHref: string;
}) {
  const t = useTranslations('alerts.customWebhook');
  const host = useRef<HTMLDivElement | null>(null);
  const view = useRef<EditorView | null>(null);

  const completions = useMemo<Completion[]>(
    () =>
      variables.map((variable, index) => ({
        label: variable.path,
        type: 'variable',
        // Listed in the order the model gives them, the fields a message is
        // built from first, rather than alphabetically.
        boost: variables.length - index,
        // The prose goes beside the name rather than in the side panel: the
        // reader is looking for the field that holds the error title, not for
        // the field whose type is string.
        detail: t(`variables.${variable.descriptionKey}`),
        info: `${variable.detail} · ${variable.example}`,
        // Accepting a field closes the expression too, so the pill forms the
        // moment the choice is made rather than after two more keystrokes.
        apply: (editor, _completion, from, to) => {
          const after = editor.state.sliceDoc(to, to + 3);
          const closing = /^\s*\}\}/.test(after) ? '' : ' }}';
          const insert = `${variable.path}${closing}`;
          const end = /^\s*\}\}/.test(after)
            ? to + after.indexOf('}}') + 2
            : from + insert.length;
          editor.dispatch({
            changes: { from, to, insert },
            selection: { anchor: end },
          });
        },
      })),
    [variables, t],
  );

  const unknownFieldLabel = t('unknownField');

  // One compartment for the editor's lifetime. `useState` rather than
  // `useRef(new Compartment())`, which would build a compartment on every
  // render and throw all but the first away.
  const [editable] = useState(() => new Compartment());

  /**
   * Everything the editor's own callbacks read, in one box.
   *
   * The editor is built once: naming any of these in the effect below would
   * tear it down and rebuild it, and a rebuild mid-word drops the caret, the
   * selection and the undo history. Writing to the box during render is what
   * React tells you not to do, so it is written after the render commits, and
   * read only from callbacks that run later than that.
   */
  const live = useRef({ onChange, onBlur, completions, unknownFieldLabel });
  useEffect(() => {
    live.current = { onChange, onBlur, completions, unknownFieldLabel };
  }, [onChange, onBlur, completions, unknownFieldLabel]);

  // Whether the reader has put the caret somewhere. Until they have, the
  // selection sits at offset 0, and inserting a field there would put it in
  // front of the opening brace of a body they never touched. An empty body
  // has nowhere else to go, so it counts as placed.
  const [caretPlaced, setCaretPlaced] = useState(() => value.trim() === '');

  // What the editor is built from, captured at mount. `useState` again, so the
  // initial value is read once instead of on every render.
  const [initial] = useState(() => ({
    doc: value,
    disabled,
    placeholder,
    ariaLabel,
  }));

  useEffect(() => {
    if (!host.current || view.current) return;

    const complete = (ctx: CompletionContext): CompletionResult | null => {
      const open = expressionAt(ctx.state.doc.toString(), ctx.pos);
      if (!open) return null;
      // Unprompted only while typing a name, or the instant `{{` was opened:
      // a list that pops up over every empty expression the caret crosses
      // would be noise.
      if (!open.word && !open.fresh && !ctx.explicit) return null;
      return {
        from: open.from,
        options: live.current.completions,
        validFor: /^[\w.]*$/,
      };
    };

    const instance = new EditorView({
      parent: host.current,
      state: EditorState.create({
        doc: initial.doc,
        extensions: [
          history(),
          json(),
          syntaxHighlighting(jsonHighlight),
          fieldPills(() => live.current.unknownFieldLabel),
          lineNumbers(),
          highlightActiveLine(),
          highlightActiveLineGutter(),
          drawSelection(),
          bracketMatching(),
          closeBrackets(),
          indentOnInput(),
          indentUnit.of('  '),
          autocompletion({
            override: [complete],
            icons: false,
            activateOnTyping: true,
          }),
          lintGutter(),
          // The dialog scrolls, and a scroll container clips anything drawn
          // inside it. The completion popup is wider than this field on
          // purpose, so it is drawn at the top of the document instead.
          tooltips({ parent: document.body }),
          keymap.of([
            ...closeBracketsKeymap,
            ...completionKeymap,
            ...historyKeymap,
            indentWithTab,
            ...defaultKeymap,
          ]),
          EditorView.lineWrapping,
          cmPlaceholder(initial.placeholder),
          editorTheme,
          editable.of(EditorView.editable.of(!initial.disabled)),
          EditorView.updateListener.of((update: ViewUpdate) => {
            if (update.docChanged) {
              live.current.onChange(update.state.doc.toString());
            }
            if (update.focusChanged && update.view.hasFocus) {
              setCaretPlaced(true);
            }
            if (update.focusChanged && !update.view.hasFocus) {
              live.current.onBlur?.();
            }
          }),
          EditorView.contentAttributes.of({
            'aria-label': initial.ariaLabel,
            role: 'textbox',
            'aria-multiline': 'true',
          }),
        ],
      }),
    });
    view.current = instance;
    return () => {
      instance.destroy();
      view.current = null;
    };
    // Built once, deliberately. `disabled` is reconfigured through a
    // compartment below and everything else is read through a ref, so this
    // array stays empty on purpose rather than by oversight.
  }, [editable, initial]);

  useEffect(() => {
    view.current?.dispatch({
      effects: editable.reconfigure(EditorView.editable.of(!disabled)),
    });
  }, [disabled, editable]);

  // A value that arrives from outside (a reset, a preset) is written in; one
  // that came from typing already matches and is left alone.
  useEffect(() => {
    const instance = view.current;
    if (!instance) return;
    const current = instance.state.doc.toString();
    if (current === value) return;
    instance.dispatch({
      changes: { from: 0, to: current.length, insert: value },
    });
  }, [value]);

  // Refusals belong on the line that caused them, underlined, with the reason
  // on hover and a marker in the gutter, where an editor puts them. The
  // preview panel says the same thing in words; this says where.
  useEffect(() => {
    const instance = view.current;
    if (!instance) return;
    const doc = instance.state.doc.toString();
    if (preview.status !== 'error' || !doc.trim()) {
      instance.dispatch(setDiagnostics(instance.state, []));
      return;
    }
    // A field the payload does not have is something this side can point at
    // exactly, pill by pill; anything else is located from the server's words.
    const unknown = findExpressions(doc).filter(
      (expression) => expression.path && !isKnownVariable(expression.path),
    );
    const diagnostics = unknown.length
      ? unknown.map(({ from, to }) => ({
          from,
          to,
          severity: 'error' as const,
          message: unknownFieldLabel,
        }))
      : [
          {
            ...locateTemplateError(doc, preview.error),
            severity: 'error' as const,
            message: preview.error,
          },
        ];
    instance.dispatch(setDiagnostics(instance.state, diagnostics));
  }, [preview, unknownFieldLabel]);

  const insertField = (variable: TemplateVariable) => {
    const instance = view.current;
    if (!instance) return;
    const { from, to } = instance.state.selection.main;
    instance.dispatch({
      changes: { from, to, insert: variable.snippet },
      selection: { anchor: from + variable.snippet.length },
      scrollIntoView: true,
    });
    // After the menu has handed focus back to its trigger.
    setTimeout(() => instance.focus(), 0);
  };

  return (
    <div
      data-disabled={disabled || undefined}
      className="group/editor overflow-hidden rounded-md border border-border-field bg-canvas transition-[border-color,box-shadow] duration-fast focus-within:border-border-brand focus-within:ring-2 focus-within:ring-ring-subtle data-disabled:opacity-50"
    >
      <EditorToolbar
        disabled={disabled}
        caretPlaced={caretPlaced}
        variables={variables}
        presets={presets}
        helpHref={helpHref}
        onPreset={onPreset}
        onInsert={insertField}
        onFormat={onFormat}
      />

      <div ref={host} />

      <PreviewPanel preview={preview} />
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/* Toolbar                                                                     */
/* -------------------------------------------------------------------------- */

/**
 * The row above the body: where to start from, what to insert, and the two
 * icon actions. Kept apart from the editor so the editor is only the editor.
 */
function EditorToolbar({
  disabled,
  caretPlaced,
  variables,
  presets,
  helpHref,
  onPreset,
  onInsert,
  onFormat,
}: {
  disabled: boolean;
  /** Insertion needs a caret; until there is one the menu stays closed. */
  caretPlaced: boolean;
  variables: readonly TemplateVariable[];
  presets: readonly TemplatePreset[];
  helpHref: string;
  onPreset: (preset: TemplatePreset) => void;
  onInsert: (variable: TemplateVariable) => void;
  onFormat: () => void;
}) {
  const t = useTranslations('alerts.customWebhook');

  // One pass over the fields, grouped for the menu.
  const byGroup = useMemo(() => {
    const groups = new Map<TemplateVariableGroup, TemplateVariable[]>();
    for (const variable of variables) {
      const list = groups.get(variable.group) ?? [];
      list.push(variable);
      groups.set(variable.group, list);
    }
    return groups;
  }, [variables]);

  return (
    <div className="flex items-center justify-between gap-2 border-border-subtle border-b bg-surface-raised px-1.5 py-1">
      <div className="flex items-center gap-0.5">
        <Menu
          trigger={
            <Button
              variant="ghost"
              size="xs"
              icon={ColumnsIcon}
              menu
              disabled={disabled}
            >
              {t('startFrom')}
            </Button>
          }
          popupClassName="w-72"
        >
          <MenuGroup>
            <MenuGroupLabel>{t('startFrom')}</MenuGroupLabel>
            <MenuActions
              actions={presets.map((preset) => ({
                id: preset.id,
                label: preset.name,
                hint: t(`presets.${preset.descriptionKey}`),
                onSelect: () => onPreset(preset),
              }))}
            />
          </MenuGroup>
        </Menu>

        <Menu
          trigger={
            <Button
              variant="ghost"
              size="xs"
              icon={NewIcon}
              menu
              disabled={disabled || !caretPlaced}
              title={caretPlaced ? undefined : t('insertFieldHint')}
            >
              {t('insertField')}
            </Button>
          }
          popupClassName="max-h-[min(28rem,70vh)] w-80"
        >
          {TEMPLATE_VARIABLE_GROUPS.map((group, index) => (
            <MenuGroup key={group}>
              {index > 0 && (
                <div aria-hidden="true" className="my-1 h-px bg-border" />
              )}
              <MenuGroupLabel>{t(`groups.${group}`)}</MenuGroupLabel>
              <MenuActions
                actions={(byGroup.get(group) ?? []).map((variable) => ({
                  id: variable.path,
                  label: variable.path,
                  hint: `${t(`variables.${variable.descriptionKey}`)} · ${variable.detail}`,
                  onSelect: () => onInsert(variable),
                }))}
              />
            </MenuGroup>
          ))}
        </Menu>
      </div>

      <div className="flex items-center gap-0.5">
        <Button
          variant="ghost"
          size="xs"
          icon={AutofixIcon}
          disabled={disabled}
          onClick={onFormat}
          aria-label={t('format')}
          title={t('format')}
        />
        <Button
          variant="ghost"
          size="xs"
          icon={HelpIcon}
          aria-label={t('help')}
          title={t('help')}
          render={<a href={helpHref} target="_blank" rel="noreferrer" />}
        />
      </div>
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/* Preview                                                                     */
/* -------------------------------------------------------------------------- */

/**
 * What the body renders to, under the editor, where the reader's eye already
 * is. The previous answer stays on screen, dimmed, while the next one is on
 * its way: an empty panel flashing between keystrokes reads as something
 * breaking.
 */
function PreviewPanel({ preview }: { preview: TemplatePreview }) {
  const t = useTranslations('alerts.customWebhook');
  const rendered = 'rendered' in preview ? preview.rendered : undefined;

  return (
    <div className="border-border-subtle border-t">
      <div className="flex items-center justify-between gap-2 px-3 pt-2 pb-1">
        <span className="flex items-baseline gap-1.5 text-column text-fg-meta">
          <PayloadIcon size="sm" className="self-center" />
          {t('preview')}
          <span className="font-sans text-hint text-fg-ghost normal-case">
            · {t('previewSample')}
          </span>
        </span>
        <PreviewStatus preview={preview} />
      </div>

      <div className="relative max-h-48 overflow-auto px-3 pb-2.5">
        {preview.status === 'idle' ? (
          <p className="py-2 text-meta text-fg-ghost">{t('previewIdle')}</p>
        ) : rendered ? (
          <pre
            className={cn(
              'whitespace-pre-wrap break-all font-mono text-mono-sm leading-relaxed transition-opacity duration-fast',
              preview.status === 'rendering' && 'opacity-50',
              preview.status === 'error' && 'opacity-40',
            )}
          >
            {highlightJson(rendered)}
          </pre>
        ) : (
          <p className="py-2 text-meta text-fg-ghost">
            {preview.status === 'rendering' ? t('previewRendering') : null}
          </p>
        )}
      </div>

      {preview.status === 'error' && (
        <div
          role="alert"
          className="animate-drop-in border-border-subtle border-t bg-danger-surface px-3 py-2 text-meta text-danger-fg"
        >
          {preview.error}
        </div>
      )}
    </div>
  );
}

/** A dot and a word: the state of the body at a glance. */
function PreviewStatus({ preview }: { preview: TemplatePreview }) {
  const t = useTranslations('alerts.customWebhook');
  if (preview.status === 'idle') return null;

  const tone = {
    rendering: {
      dot: 'animate-pulse bg-fg-ghost',
      text: 'text-fg-muted',
      label: t('previewRendering'),
    },
    ok: {
      dot: 'bg-success',
      text: 'text-fg-muted',
      label: t('previewOk'),
    },
    error: {
      dot: 'bg-danger',
      text: 'text-danger-fg',
      label: t('previewError'),
    },
    unavailable: {
      dot: 'bg-fg-disabled',
      text: 'text-fg-muted',
      label: t('previewUnavailable'),
    },
  }[preview.status];

  return (
    <span
      className={cn(
        'flex items-center gap-1.5 text-hint transition-colors duration-fast',
        tone.text,
      )}
      aria-live="polite"
    >
      <span
        className={cn(
          'size-1.5 rounded-pill transition-colors duration-fast',
          tone.dot,
        )}
      />
      {tone.label}
    </span>
  );
}

/**
 * JSON in the preview, coloured the same as JSON in the editor. A regex pass
 * rather than a second CodeMirror instance: the preview is read, not edited,
 * and it is a few lines.
 */
const JSON_TOKEN =
  /("(?:\\.|[^"\\])*")(\s*:)?|(-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?)|\b(true|false)\b|\b(null)\b/g;

/** The colour of a token, by what it is. */
function tokenStyle(match: RegExpMatchArray): React.CSSProperties {
  const [, string, colon, number, bool] = match;
  if (string !== undefined) {
    return { color: colon ? 'var(--chart-1)' : 'var(--chart-2)' };
  }
  if (number !== undefined || bool !== undefined) {
    return { color: 'var(--chart-4)' };
  }
  return { color: 'var(--fg-subtle)' };
}

function highlightJson(text: string): React.ReactNode[] {
  const nodes: React.ReactNode[] = [];
  let last = 0;
  for (const match of text.matchAll(JSON_TOKEN)) {
    const at = match.index;
    if (at > last) nodes.push(text.slice(last, at));
    // A key's colon is part of the match but not of the key's colour.
    const token = match[2] ? (match[1] as string) : match[0];
    nodes.push(
      <span key={at} style={tokenStyle(match)}>
        {token}
      </span>,
    );
    if (match[2]) nodes.push(match[2]);
    last = at + match[0].length;
  }
  if (last < text.length) nodes.push(text.slice(last));
  return nodes;
}

/* -------------------------------------------------------------------------- */
/* Pills                                                                       */
/* -------------------------------------------------------------------------- */

/**
 * `{{ issue.title }}` drawn as one thing. The DOM is reused across updates
 * while `eq` holds, which is also what makes the entrance animation play once,
 * on the pill that was just inserted, and not on every pill on every keystroke.
 */
class FieldPillWidget extends WidgetType {
  constructor(
    readonly path: string,
    readonly known: boolean,
    readonly unknownLabel: string,
  ) {
    super();
  }

  eq(other: FieldPillWidget) {
    return other.path === this.path && other.known === this.known;
  }

  toDOM() {
    const pill = document.createElement('span');
    pill.className = this.known
      ? 'cm-field-pill'
      : 'cm-field-pill cm-field-pill-unknown';
    pill.textContent = this.path;
    pill.title = this.known ? this.path : this.unknownLabel;
    pill.setAttribute('aria-label', this.path);
    return pill;
  }

  ignoreEvent() {
    return false;
  }
}

/** Marks an expression that is not a plain field, so it reads as intended. */
const expressionMark = Decoration.mark({ class: 'cm-template-expression' });

/**
 * Pills for plain fields, a tint for everything else between `{{` and `}}`.
 * Pills are atomic: the caret steps over them and Backspace takes the whole
 * thing, so there is no such state as "half a field".
 */
function fieldPills(unknownLabel: () => string) {
  const plugin = ViewPlugin.fromClass(
    class {
      decorations: DecorationSet;
      constructor(view: EditorView) {
        this.decorations = decorate(view, unknownLabel());
      }
      update(update: ViewUpdate) {
        if (
          update.docChanged ||
          update.viewportChanged ||
          update.selectionSet
        ) {
          this.decorations = decorate(update.view, unknownLabel());
        }
      }
    },
    {
      decorations: (instance) => instance.decorations,
      provide: (instance) =>
        EditorView.atomicRanges.of(
          (view) => view.plugin(instance)?.decorations ?? Decoration.none,
        ),
    },
  );
  return plugin;
}

/**
 * An expression the caret is inside is being written, and a pill would swallow
 * it mid-word: `{{ i}}` (the braces auto-closed) would become atomic on the
 * first letter and push the caret out. So it stays text until the caret
 * leaves, which is also what lets `{{` completion work at all.
 */
function decorate(view: EditorView, unknownLabel: string): DecorationSet {
  const ranges: ReturnType<typeof expressionMark.range>[] = [];
  const caret = view.state.selection.main.head;
  for (const { from, to, path } of findExpressions(view.state.doc.toString())) {
    const writing = caret > from && caret < to;
    ranges.push(
      path && !writing
        ? Decoration.replace({
            widget: new FieldPillWidget(
              path,
              isKnownVariable(path),
              unknownLabel,
            ),
          }).range(from, to)
        : expressionMark.range(from, to),
    );
  }
  return Decoration.set(ranges, true);
}

/* -------------------------------------------------------------------------- */
/* Theme                                                                       */
/* -------------------------------------------------------------------------- */

/**
 * JSON's own colours. The editor ships none of its own, so without this the
 * body is one flat grey block and the point of using an editor is lost.
 */
const jsonHighlight = HighlightStyle.define([
  { tag: tags.propertyName, color: 'var(--chart-1)' },
  { tag: tags.string, color: 'var(--chart-2)' },
  { tag: tags.number, color: 'var(--chart-4)' },
  { tag: tags.bool, color: 'var(--chart-4)' },
  { tag: tags.null, color: 'var(--fg-subtle)' },
  { tag: tags.separator, color: 'var(--fg-subtle)' },
  { tag: tags.brace, color: 'var(--fg-subtle)' },
  { tag: tags.squareBracket, color: 'var(--fg-subtle)' },
]);

const editorTheme = EditorView.theme({
  '&': { fontSize: '12px' },
  '&.cm-focused': { outline: 'none' },
  '.cm-content': {
    fontFamily: 'var(--font-mono, ui-monospace, monospace)',
    padding: '8px 0',
    minHeight: '11rem',
    caretColor: 'var(--fg)',
  },
  '.cm-scroller': {
    maxHeight: '22rem',
    lineHeight: '1.7',
    fontFamily: 'var(--font-mono, ui-monospace, monospace)',
  },
  '.cm-line': { padding: '0 10px' },
  '.cm-gutters': {
    background: 'transparent',
    border: 'none',
    borderRight: '1px solid var(--border-subtle)',
    color: 'var(--fg-subtle)',
    paddingRight: '2px',
    userSelect: 'none',
  },
  '.cm-lintRange-error': {
    backgroundImage: 'none',
    textDecoration: 'underline wavy var(--danger-fg)',
    textUnderlineOffset: '3px',
  },
  '.cm-tooltip-lint': { maxWidth: 'min(28rem, 85vw)' },
  '.cm-diagnostic-error': {
    borderLeftColor: 'var(--danger-fg)',
    fontFamily: 'inherit',
  },
  '.cm-lineNumbers .cm-gutterElement': {
    padding: '0 6px 0 10px',
    minWidth: '1.5rem',
  },
  '.cm-activeLine': {
    background: 'color-mix(in oklch, var(--fg) 4%, transparent)',
  },
  '.cm-activeLineGutter': {
    background: 'transparent',
    color: 'var(--fg)',
  },
  '.cm-placeholder': { color: 'var(--fg-subtle)' },
  '&.cm-focused .cm-cursor': { borderLeftColor: 'var(--fg)' },
  '.cm-selectionBackground, ::selection': {
    background: 'var(--surface-selected)',
  },
  '&.cm-focused .cm-selectionBackground': {
    background: 'var(--surface-selected)',
  },
  '.cm-template-expression': {
    color: 'var(--fg-brand)',
    background: 'color-mix(in oklch, var(--fg-brand) 10%, transparent)',
    borderRadius: '3px',
  },
  '.cm-field-pill': {
    display: 'inline-block',
    verticalAlign: 'baseline',
    margin: '0 1px',
    padding: '0 6px',
    borderRadius: '5px',
    lineHeight: '1.45',
    fontFamily: 'var(--font-mono, ui-monospace, monospace)',
    fontSize: '11px',
    fontWeight: '500',
    color: 'var(--fg-brand)',
    background: 'color-mix(in oklch, var(--fg-brand) 12%, transparent)',
    boxShadow:
      'inset 0 0 0 1px color-mix(in oklch, var(--fg-brand) 28%, transparent)',
    cursor: 'default',
    userSelect: 'none',
    whiteSpace: 'nowrap',
    animation: 'cm-field-pill-in 180ms cubic-bezier(0.2, 0, 0, 1)',
    transition: 'background-color 150ms, box-shadow 150ms',
  },
  '.cm-field-pill:hover': {
    background: 'color-mix(in oklch, var(--fg-brand) 20%, transparent)',
    boxShadow:
      'inset 0 0 0 1px color-mix(in oklch, var(--fg-brand) 45%, transparent)',
  },
  '.cm-field-pill-unknown': {
    color: 'var(--danger-fg)',
    background: 'color-mix(in oklch, var(--danger-fg) 10%, transparent)',
    boxShadow:
      'inset 0 0 0 1px color-mix(in oklch, var(--danger-fg) 40%, transparent)',
    textDecoration: 'underline dotted',
    textUnderlineOffset: '2px',
  },
  '.cm-field-pill-unknown:hover': {
    background: 'color-mix(in oklch, var(--danger-fg) 16%, transparent)',
    boxShadow:
      'inset 0 0 0 1px color-mix(in oklch, var(--danger-fg) 55%, transparent)',
  },
  '@keyframes cm-field-pill-in': {
    from: { transform: 'scale(0.9)', opacity: '0' },
    to: { transform: 'scale(1)', opacity: '1' },
  },
  '.cm-tooltip-autocomplete': {
    // Wide enough for a name and its description on one line; the popup is
    // free to reach past the field, which is narrower than the sentence.
    minWidth: 'min(28rem, 85vw)',
    maxWidth: 'min(34rem, 90vw)',
  },
  // CodeMirror's base theme puts the whole list in monospace; only the
  // field name should be, the description is a sentence.
  '.cm-tooltip.cm-tooltip-autocomplete > ul': {
    maxHeight: '13rem',
    fontFamily: 'var(--font-sans, system-ui, sans-serif)',
  },
  '.cm-tooltip': {
    fontFamily: 'var(--font-sans, system-ui, sans-serif)',
    background: 'var(--surface-floating)',
    color: 'var(--fg)',
    border: '1px solid var(--border-subtle)',
    borderRadius: '8px',
    boxShadow: 'var(--shadow-overlay)',
    fontSize: '12px',
  },
  '.cm-tooltip-autocomplete > ul > li[aria-selected]': {
    background: 'var(--surface-selected)',
    color: 'var(--fg)',
  },
  '.cm-tooltip-autocomplete > ul > li': {
    padding: '4px 10px',
    display: 'flex',
    alignItems: 'baseline',
    gap: '0.75rem',
    justifyContent: 'space-between',
  },
  '.cm-completionLabel': {
    fontFamily: 'var(--font-mono, ui-monospace, monospace)',
  },
  '.cm-completionDetail': {
    color: 'var(--fg-subtle)',
    fontStyle: 'normal',
    fontFamily: 'inherit',
    whiteSpace: 'nowrap',
  },
  '.cm-completionInfo': {
    background: 'var(--surface-floating)',
    border: '1px solid var(--border-subtle)',
    borderRadius: '8px',
    padding: '6px 8px',
    maxWidth: '18rem',
  },
});
