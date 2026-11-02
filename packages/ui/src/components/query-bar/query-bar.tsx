import type { ColumnFiltersState } from '@tanstack/react-table';
import type { MouseEvent as ReactMouseEvent } from 'react';
import { focusRingWithin } from '../../lib/focus';
import { dropIn, interactiveTransition } from '../../lib/motion';
import { tv } from '../../lib/tv';
import { Count } from '../count/count';
import { useDataTableLabels } from '../data-table/labels';
import {
  CloseIcon,
  FacetsIcon,
  ResolveIcon,
  SearchIcon,
} from '../icon/icon-catalog';
import { Kbd } from '../kbd/kbd';
import type { TagTone } from '../tag/tag';
import type { QueryField } from './query-bar-parts';
import { type Suggestion, useQueryBar } from './use-query-bar';

/**
 * The query bar: one line that holds the whole question.
 *
 * Filters live in it as `key:value` chips and free text rides after them, so
 * the bar always reads back as the exact string `parseFilterQuery` accepts --
 * what you see is what a teammate can paste. It edits the same
 * `ColumnFiltersState` the column header panels edit: tick "Warning" in the
 * Level column and a chip appears here; delete the chip and the column's
 * funnel goes out. One state, two views of it.
 *
 * The autocomplete works in two phases, the way GitHub's does. Typing offers
 * the *fields* that can be asked about; choosing one writes `level:` and the
 * popup turns to that field's *values*, ticking live as they are chosen.
 * Free text never needs a phase: whatever no key claims is the search, and
 * Enter sends it.
 */
const queryBar = tv({
  slots: {
    root: 'relative min-w-0 flex-1',
    /*
     * The bar wraps rather than clips. A fixed-height field with
     * `overflow-hidden` chips read fine until a phone: the third chip
     * disappeared with no trace, and what cannot be seen cannot be removed.
     * `min-h` keeps the resting bar at exactly the 36 px it always was; only
     * a second row of chips makes it taller, and the vertical padding is
     * what keeps the wrapped rows off the border.
     */
    field: [
      'flex min-h-control-lg w-full min-w-0 flex-wrap items-center gap-1.5',
      'rounded-md border border-border bg-surface ps-2.5 pe-1.5 py-1',
      'cursor-text',
      interactiveTransition,
      'hover:border-border-strong',
      focusRingWithin,
    ],
    // `contents`: the chips are flex items of the field itself, so they
    // share its wrapping instead of clipping inside a nested row.
    chips: 'contents',
    chip: [
      'flex h-chip max-w-full shrink-0 items-center rounded-xs bg-surface-chip',
      'ps-2 pe-0.5 font-mono text-mono-sm',
      'animate-swap-in motion-reduce:animate-none',
    ],
    chipKey: 'shrink-0 text-fg-subtle',
    // A value longer than the viewport keeps its chip on one line and says
    // the rest with an ellipsis.
    chipValue: 'ms-0.5 min-w-0 truncate text-fg',
    chipRemove: [
      'ms-0.5 flex size-4.5 items-center justify-center rounded-2xs',
      'text-fg-ghost outline-none',
      interactiveTransition,
      'hover:bg-surface-selected hover:text-fg',
      'focus-visible:bg-surface-selected focus-visible:text-fg',
    ],
    // Its own height, not `h-full`: in a wrapped field, full height would be
    // every row at once and the caret would sit in the middle of them.
    input: [
      'h-chip min-w-24 flex-1 bg-transparent text-control text-fg',
      'outline-none placeholder:text-fg-placeholder',
    ],
    clear: [
      'flex size-6 shrink-0 items-center justify-center rounded-sm',
      'text-fg-ghost outline-none',
      interactiveTransition,
      'hover:bg-surface-selected hover:text-fg',
      'focus-visible:bg-surface-selected focus-visible:text-fg',
    ],
    popup: [
      'absolute inset-x-0 top-full z-50 mt-1.5',
      // The list is what scrolls: the keyboard footer stays put, because the
      // person scrolling a long value list is exactly who needs it.
      'flex max-h-80 flex-col overflow-hidden',
      'rounded-2xl border border-border bg-surface-floating p-1.5',
      'shadow-overlay',
      dropIn,
    ],
    list: 'min-h-0 flex-1 overflow-x-hidden overflow-y-auto',
    groupLabel: 'px-2.5 pt-2 pb-1 text-column text-fg-meta',
    option: [
      'flex h-menu-item shrink-0 cursor-default items-center gap-2.5',
      'rounded-sm px-2.5 text-control text-fg-muted select-none',
      'transition-none outline-none',
      'data-[highlighted=true]:bg-surface-selected data-[highlighted=true]:text-fg',
    ],
    optionIcon: 'shrink-0 text-fg-ghost',
    optionBox: [
      'flex size-3.5 shrink-0 items-center justify-center',
      'rounded-xs border border-border-control text-fg-on-brand',
      'data-[ticked=true]:border-surface-brand data-[ticked=true]:bg-surface-brand',
    ],
    optionDot: 'size-dot shrink-0 rounded-pill',
    optionLabel: 'min-w-0 flex-1 truncate text-start',
    optionHint: 'ms-auto shrink-0 truncate text-fg-ghost text-hint',
    skeleton: 'mx-2.5 my-2 h-3 animate-pulse rounded-xs bg-surface-chip',
    hints: [
      'mt-1 flex shrink-0 items-center gap-3.5 border-border-subtle border-t',
      'px-2.5 pt-2 pb-1 font-mono text-fg-ghost text-mono-sm',
    ],
  },
});

const styles = queryBar();

const TONE_DOT: Record<TagTone, string> = {
  error: 'bg-sev-error',
  warning: 'bg-sev-warning',
  info: 'bg-sev-info',
  brand: 'bg-surface-brand',
  neutral: 'bg-fg-ghost',
};

function suggestionKey(suggestion: Suggestion): string {
  if (suggestion.kind === 'field') return `field-${suggestion.field.key}`;
  if (suggestion.kind === 'value') return `value-${suggestion.option.value}`;
  return 'search';
}

interface SuggestionRowProps {
  suggestion: Suggestion;
  optionId: string;
  highlighted: boolean;
  ticked: boolean;
  onHighlight: () => void;
  onSelect: () => void;
}

/** One row of the popup: a field to pick, a value to tick, or the search itself. */
function SuggestionRow({
  suggestion,
  optionId,
  highlighted,
  ticked,
  onHighlight,
  onSelect,
}: SuggestionRowProps) {
  const shared = {
    id: optionId,
    'data-highlighted': highlighted,
    className: styles.option(),
    onMouseEnter: onHighlight,
    // Fires before the input's blur: the click must win.
    onMouseDown: (event: ReactMouseEvent) => event.preventDefault(),
    onClick: onSelect,
  };

  if (suggestion.kind === 'field') {
    const Icon = suggestion.field.icon ?? FacetsIcon;
    return (
      <div
        {...shared}
        role="option"
        // Steered from the input via aria-activedescendant.
        tabIndex={-1}
        aria-selected={false}
      >
        <Icon size="lg" className={styles.optionIcon()} />
        <span className={styles.optionLabel()}>{suggestion.field.label}</span>
        <span className={styles.optionHint()}>
          {suggestion.field.description ?? `${suggestion.field.key}:`}
        </span>
      </div>
    );
  }

  if (suggestion.kind === 'value') {
    return (
      <div {...shared} role="option" tabIndex={-1} aria-selected={ticked}>
        <span
          aria-hidden="true"
          data-ticked={ticked}
          className={styles.optionBox()}
        >
          {ticked ? <ResolveIcon size="sm" aria-hidden="true" /> : null}
        </span>
        {suggestion.option.tone ? (
          <span
            aria-hidden="true"
            className={styles.optionDot({
              className: TONE_DOT[suggestion.option.tone],
            })}
          />
        ) : null}
        <span className={styles.optionLabel()}>{suggestion.option.label}</span>
        {suggestion.option.hint ? (
          <span className={styles.optionHint()}>{suggestion.option.hint}</span>
        ) : null}
        <Count>{suggestion.option.count}</Count>
      </div>
    );
  }

  return (
    <div {...shared} role="option" tabIndex={-1} aria-selected={false}>
      <SearchIcon size="lg" className={styles.optionIcon()} />
      <span className={styles.optionLabel()}>Search “{suggestion.text}”</span>
      <Kbd>⏎</Kbd>
    </div>
  );
}

SuggestionRow.displayName = 'SuggestionRow';

interface SuggestionsPopupProps {
  id: string;
  listboxId: string;
  activeField: QueryField | undefined;
  suggestions: Suggestion[];
  optionsPending: boolean;
  highlightIndex: number;
  activeFieldSelected: Set<string>;
  onHighlight: (index: number) => void;
  onSelect: (suggestion: Suggestion) => void;
}

/** The floating panel under the bar: field or value suggestions, and the footer's hints. */
function SuggestionsPopup({
  id,
  listboxId,
  activeField,
  suggestions,
  optionsPending,
  highlightIndex,
  activeFieldSelected,
  onHighlight,
  onSelect,
}: SuggestionsPopupProps) {
  const labels = useDataTableLabels();
  return (
    <div className={styles.popup()} data-side="bottom">
      {activeField && activeField.variant === 'options' ? (
        <div className={styles.groupLabel()}>{activeField.label}</div>
      ) : null}
      {!activeField && suggestions.some((s) => s.kind === 'field') ? (
        <div className={styles.groupLabel()}>{labels.filterBy}</div>
      ) : null}

      <div className={styles.list()}>
        {optionsPending ? (
          <div aria-hidden="true">
            {[0, 1, 2].map((line) => (
              <div key={line} className={styles.skeleton()} />
            ))}
          </div>
        ) : null}

        <div
          role="listbox"
          id={listboxId}
          aria-label={labels.suggestions}
          aria-busy={optionsPending || undefined}
        >
          {suggestions.map((suggestion, index) => (
            <SuggestionRow
              key={suggestionKey(suggestion)}
              suggestion={suggestion}
              optionId={`${id}-option-${index}`}
              highlighted={index === highlightIndex}
              ticked={
                suggestion.kind === 'value' &&
                activeFieldSelected.has(suggestion.option.value)
              }
              onHighlight={() => onHighlight(index)}
              onSelect={() => onSelect(suggestion)}
            />
          ))}
        </div>

        {activeField && activeField.variant === 'range' ? (
          <div className={styles.option()}>
            <span className={styles.optionLabel()}>
              Type a range: {activeField.key}:100..500, then
            </span>
            <Kbd>⏎</Kbd>
          </div>
        ) : null}
        {activeField && activeField.variant === 'text' ? (
          <div className={styles.option()}>
            <span className={styles.optionLabel()}>Type a value, then</span>
            <Kbd>⏎</Kbd>
          </div>
        ) : null}

        {!optionsPending &&
        suggestions.length === 0 &&
        activeField?.variant === 'options' ? (
          <div className="px-2.5 py-3 text-center text-control text-fg-ghost">
            Nothing matches
          </div>
        ) : null}
      </div>

      <span aria-hidden="true" className={styles.hints()}>
        <span>↑↓ navigate</span>
        <span>⏎ select</span>
        <span>esc closes</span>
      </span>
    </div>
  );
}

SuggestionsPopup.displayName = 'SuggestionsPopup';

export interface QueryBarProps {
  fields: QueryField[];
  filters: ColumnFiltersState;
  search: string;
  onChange: (next: { filters: ColumnFiltersState; search: string }) => void;
  placeholder?: string;
  /** Named in the field's accessible name; there may be several bars. */
  label?: string;
  className?: string;
}

export function QueryBar({
  fields,
  filters,
  search,
  onChange,
  placeholder,
  label,
  className,
}: QueryBarProps) {
  const labels = useDataTableLabels();
  const {
    id,
    inputRef,
    rootRef,
    draft,
    setDraft,
    open,
    setOpen,
    pickedField,
    setPickedKey,
    activeField,
    setHighlighted,
    highlightIndex,
    suggestions,
    optionsPending,
    activeFieldSelected,
    chips,
    listboxId,
    showClear,
    apply,
    removeFilter,
    clearAll,
    onKeyDown,
  } = useQueryBar({ fields, filters, search, onChange });

  return (
    <div ref={rootRef} className={styles.root({ className })}>
      {/* The field is a click target for the input inside it -- the whole
          bar must feel like one text field, chips included. */}
      <div className={styles.field()} onClick={() => inputRef.current?.focus()}>
        <SearchIcon size="lg" className="shrink-0 text-fg-ghost" />

        {chips.length ? (
          <span className={styles.chips()}>
            {chips.map((chip) => (
              <span key={`${chip.key}:${chip.value}`} className={styles.chip()}>
                <span className={styles.chipKey()}>{chip.key}:</span>
                <span className={styles.chipValue()}>{chip.value}</span>
                <button
                  type="button"
                  aria-label={`Remove ${chip.label} filter`}
                  className={styles.chipRemove()}
                  onClick={() => removeFilter(chip.key)}
                >
                  <CloseIcon size="sm" aria-hidden="true" />
                </button>
              </span>
            ))}
          </span>
        ) : null}

        <input
          ref={inputRef}
          type="text"
          role="combobox"
          aria-expanded={open}
          aria-controls={listboxId}
          aria-activedescendant={
            open && suggestions[highlightIndex]
              ? `${id}-option-${highlightIndex}`
              : undefined
          }
          aria-autocomplete="list"
          aria-label={label ?? labels.filterAndSearch}
          value={draft}
          placeholder={
            pickedField
              ? `${pickedField.label}…`
              : chips.length
                ? labels.addFilter
                : (placeholder ?? labels.filterAndSearch)
          }
          autoComplete="off"
          spellCheck={false}
          className={styles.input()}
          onChange={(event) => {
            setDraft(event.target.value);
            setHighlighted(0);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onBlur={(event) => {
            // Leaving for the popup is not leaving. And leaving commits
            // nothing: half a question walked away from is not a question.
            if (!rootRef.current?.contains(event.relatedTarget)) {
              setOpen(false);
              setPickedKey(null);
            }
          }}
          onKeyDown={onKeyDown}
        />

        {showClear ? (
          <button
            type="button"
            aria-label={labels.clearAll}
            className={styles.clear()}
            onClick={clearAll}
          >
            <CloseIcon size="md" aria-hidden="true" />
          </button>
        ) : null}
      </div>

      {open ? (
        <SuggestionsPopup
          id={id}
          listboxId={listboxId}
          activeField={activeField}
          suggestions={suggestions}
          optionsPending={optionsPending}
          highlightIndex={highlightIndex}
          activeFieldSelected={activeFieldSelected}
          onHighlight={setHighlighted}
          onSelect={apply}
        />
      ) : null}
    </div>
  );
}

QueryBar.displayName = 'QueryBar';
