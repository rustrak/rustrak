import type { Meta, StoryObj } from '@storybook/react-vite';
import type { Row } from '@tanstack/react-table';
import { useMemo, useState } from 'react';
import { expect, fn, userEvent, waitFor, within } from 'storybook/test';
import { Button } from '../button/button';
import {
  AssignIcon,
  DeleteIcon,
  MuteIcon,
  ResolveIcon,
} from '../icon/icon-catalog';
import type { MenuAction } from '../menu/menu-parts';
import { QueryBar } from '../query-bar/query-bar';
import { queryFieldsFromColumns } from '../query-bar/query-bar-parts';
import { Tag, type TagTone } from '../tag/tag';
import { Text } from '../text/text';
import { TooltipProvider } from '../tooltip/tooltip';
import { DataTableColumnsButton } from './columns-menu';
import { DataTable } from './data-table';
import {
  createDataTableColumnHelper,
  type DataTableFeatures,
  type FilterOption,
} from './features';
import { type DataTableQuery, emptyTableQuery } from './query';
import { useDataTable } from './use-data-table';

/**
 * The stories drive the table exactly the way the dashboard will: the
 * component is fully manual, so a tiny in-memory "server" below filters,
 * sorts and slices the fixture from the same `DataTableQuery` the real
 * server will receive. If a story works, the wiring contract works.
 */

interface Issue {
  id: string;
  level: 'fatal' | 'error' | 'warning' | 'info';
  title: string;
  culprit: string;
  events: number;
  users: number;
  /** Minutes since last seen; the fixture has no clock. */
  seenMinutesAgo: number;
}

const LEVEL_TONE: Record<Issue['level'], TagTone> = {
  fatal: 'error',
  error: 'error',
  warning: 'warning',
  info: 'info',
};

const LEVEL_OPTIONS: FilterOption[] = [
  { value: 'fatal', label: 'Fatal', tone: 'error', count: 5 },
  { value: 'error', label: 'Error', tone: 'error', count: 97 },
  { value: 'warning', label: 'Warning', tone: 'warning', count: 31 },
  { value: 'info', label: 'Info', tone: 'info', count: 10 },
];

const TITLES: Array<[Issue['level'], string, string]> = [
  [
    'fatal',
    'ConnectionTimeout: pool exhausted after 30000ms',
    'db.pool in acquire',
  ],
  [
    'error',
    "TypeError: Cannot read properties of undefined (reading 'total')",
    'CheckoutSummary in render',
  ],
  [
    'error',
    'PaymentDeclined: issuer rejected authorization',
    'payments.charge in capture',
  ],
  [
    'error',
    'FetchError: ECONNREFUSED 10.0.3.12:6379',
    'cache.redis in connect',
  ],
  [
    'warning',
    'ValidationError: coupon code not applicable',
    'cart.applyCoupon',
  ],
  ['error', 'UnhandledRejection: socket hang up', 'webhooks.dispatch'],
  ['info', 'DeprecationWarning: legacy shipping API called', 'shipping.quote'],
  ['error', 'NotFoundError: order 88213 missing on refund', 'orders.refund'],
  ['warning', 'SlowQuery: SELECT … 2400ms', 'db.query in report'],
  ['error', 'JSONDecodeError: unexpected token < at 1:1', 'api.parseBody'],
];

/** Deterministic fixture: the same 60 rows on every run, no clock, no RNG. */
const ISSUES: Issue[] = Array.from({ length: 60 }, (_, index) => {
  const [level, title, culprit] = TITLES[index % TITLES.length] as [
    Issue['level'],
    string,
    string,
  ];
  return {
    id: `RUSTRAK-${1000 + index}`,
    level,
    title,
    culprit: `${culprit} · RUSTRAK-${1000 + index}`,
    events: ((index * 397) % 4200) + 12,
    users: ((index * 131) % 900) + 3,
    seenMinutesAgo: ((index * 47) % 720) + 1,
  };
});

const SEVERITY = { info: 0, warning: 1, error: 2, fatal: 3 } as const;

/** Whether `issue` passes one filter, keyed by the column the filter names. */
const MATCHERS: Record<string, (issue: Issue, value: unknown) => boolean> = {
  level: (issue, value) => (value as string[]).includes(issue.level),

  events: (issue, value) => inRange(issue.events, value),
  users: (issue, value) => inRange(issue.users, value),

  culprit: (issue, value) => contains(issue.culprit, value as string),
  title: (issue, value) => contains(issue.title, value as string),

  // The one filter whose option value is not what it compares against: the
  // rows carry an age and the options carry a cutoff.
  seen: (issue, value) =>
    issue.seenMinutesAgo <= Number((value as string[])[0]),
};

const inRange = (n: number, value: unknown): boolean => {
  const [min, max] = value as [number | null, number | null];
  return (min === null || n >= min) && (max === null || n <= max);
};

const contains = (haystack: string, needle: string): boolean =>
  haystack.toLowerCase().includes(needle.toLowerCase());

/** A filter naming a column with no matcher is ignored, not fatal. */
function matchesFilters(issue: Issue, query: DataTableQuery): boolean {
  return query.filters.every(
    (filter) => MATCHERS[filter.id]?.(issue, filter.value) ?? true,
  );
}

/** Free text searches the two columns a reader would expect it to. */
function matchesSearch(issue: Issue, search: string): boolean {
  if (!search) return true;
  return contains(`${issue.title} ${issue.culprit}`, search);
}

/** Column ids are the URL's names, and two of them are not field names. */
function sortValue(issue: Issue, id: string): number | string {
  if (id === 'level') return SEVERITY[issue.level];
  if (id === 'seen') return issue.seenMinutesAgo;
  return issue[id as keyof Issue] as number | string;
}

function sortRows(rows: Issue[], query: DataTableQuery): Issue[] {
  const sort = query.sorting[0];
  if (!sort) return rows;

  return [...rows].sort((a, b) => {
    const left = sortValue(a, sort.id);
    const right = sortValue(b, sort.id);
    const order =
      typeof left === 'number' && typeof right === 'number'
        ? left - right
        : String(left).localeCompare(String(right));
    return sort.desc ? -order : order;
  });
}

/**
 * What the Rust server will do, done to the fixture: the story's backend.
 *
 * Worth reading as documentation rather than as scaffolding -- this is the
 * filter, sort and page behaviour the real endpoint is expected to match.
 */
function fakeServer(query: DataTableQuery): { rows: Issue[]; total: number } {
  const matched = ISSUES.filter(
    (issue) =>
      matchesFilters(issue, query) && matchesSearch(issue, query.search),
  );
  const sorted = sortRows(matched, query);

  const { pageIndex, pageSize } = query.pagination;
  return {
    rows: sorted.slice(pageIndex * pageSize, (pageIndex + 1) * pageSize),
    total: matched.length,
  };
}

function formatSeen(minutes: number): string {
  if (minutes < 60) return `${minutes} min`;
  if (minutes < 24 * 60) return `${Math.floor(minutes / 60)} h`;
  return `${Math.floor(minutes / (24 * 60))} d`;
}

const columnHelper = createDataTableColumnHelper<Issue>();

const columns = columnHelper.columns([
  columnHelper.accessor('level', {
    id: 'level',
    header: 'Level',
    cell: ({ getValue }) => (
      <Tag tone={LEVEL_TONE[getValue()]}>{getValue()}</Tag>
    ),
    meta: {
      label: 'Level',
      width: 88,
      sortLabels: { asc: 'Least severe first', desc: 'Most severe first' },
      filter: { variant: 'options', options: LEVEL_OPTIONS },
    },
  }),
  columnHelper.accessor('title', {
    id: 'title',
    header: 'Issue',
    enableHiding: false,
    cell: ({ row }) => (
      <span className="flex min-w-0 flex-col gap-0.5 py-2">
        <Text variant="value" truncate>
          {row.original.title}
        </Text>
        <Text variant="mono-sm" tone="ghost" truncate>
          {row.original.culprit}
        </Text>
      </span>
    ),
    meta: {
      label: 'Issue',
      sortLabels: { asc: 'A to Z', desc: 'Z to A' },
      filter: { variant: 'text', placeholder: 'Title contains…' },
    },
  }),
  columnHelper.accessor('culprit', {
    id: 'culprit',
    header: 'Culprit',
    cell: ({ getValue }) => (
      <Text variant="mono-sm" tone="subtle" truncate>
        {getValue()}
      </Text>
    ),
    meta: {
      label: 'Culprit',
      width: 180,
      filter: { variant: 'text', placeholder: 'Contains…' },
    },
  }),
  columnHelper.accessor('events', {
    id: 'events',
    header: 'Events',
    cell: ({ getValue }) => getValue().toLocaleString('en-US'),
    meta: {
      label: 'Events',
      width: 96,
      align: 'end',
      numeric: true,
      sortLabels: { asc: 'Fewest events', desc: 'Most events' },
      filter: { variant: 'range', min: 0, unit: 'events' },
    },
  }),
  columnHelper.accessor('users', {
    id: 'users',
    header: 'Users',
    cell: ({ getValue }) => getValue().toLocaleString('en-US'),
    meta: {
      label: 'Users',
      width: 88,
      align: 'end',
      numeric: true,
      sortLabels: { asc: 'Fewest users', desc: 'Most users' },
      filter: { variant: 'range', min: 0, unit: 'users' },
    },
  }),
  columnHelper.accessor('seenMinutesAgo', {
    id: 'seen',
    header: 'Last seen',
    cell: ({ getValue }) => formatSeen(getValue()),
    meta: {
      label: 'Last seen',
      width: 100,
      align: 'end',
      sortLabels: { asc: 'Oldest first', desc: 'Newest first' },
      // A date narrows to a window, and exactly one window holds at a time.
      filter: {
        variant: 'options',
        multiple: false,
        options: [
          { value: '60', label: 'Last hour' },
          { value: '1440', label: 'Last 24 h' },
          { value: '10080', label: 'Last 7 d' },
        ],
      },
    },
  }),
]);

const fields = queryFieldsFromColumns(columns);

const issueRowMenu = (row: Row<DataTableFeatures, Issue>): MenuAction[] => [
  {
    id: 'resolve',
    label: 'Resolve',
    icon: ResolveIcon,
    shortcut: 'R',
    onSelect: () => console.info('resolve', row.id),
  },
  {
    id: 'mute',
    label: 'Mute',
    icon: MuteIcon,
    onSelect: () => console.info('mute', row.id),
  },
  {
    id: 'assign',
    label: 'Assign',
    icon: AssignIcon,
    onSelect: () => console.info('assign', row.id),
  },
  {
    id: 'delete',
    label: 'Delete',
    icon: DeleteIcon,
    tone: 'danger',
    separated: true,
    onSelect: () => console.info('delete', row.id),
  },
];

function IssuesTable({
  loading = false,
  data = undefined as Issue[] | undefined,
  onRowClick = (row: { id: string }) => console.info('open', row.id),
}) {
  const [query, setQuery] = useState<DataTableQuery>(() => ({
    ...emptyTableQuery(10),
    sorting: [{ id: 'events', desc: true }],
  }));

  const page = useMemo(
    () => (data ? { rows: data, total: data.length } : fakeServer(query)),
    [data, query],
  );

  const table = useDataTable({
    data: page.rows,
    columns,
    rowCount: page.total,
    query,
    onQueryChange: (updater) => setQuery((previous) => updater(previous)),
    getRowId: (issue) => issue.id,
    enableSelection: true,
    rowMenu: issueRowMenu,
  });

  return (
    <TooltipProvider>
      <div className="flex h-140 flex-col gap-3">
        {/* The toolbar of the issues screen: the bar and the table edit the
            same filters, so a tick in a column panel appears here as a chip
            and a removed chip puts the column's funnel out. */}
        <div className="flex items-center gap-2">
          <QueryBar
            fields={fields}
            filters={query.filters}
            search={query.search}
            onChange={(next) =>
              setQuery((previous) => ({
                ...previous,
                filters: next.filters,
                search: next.search,
                pagination: { ...previous.pagination, pageIndex: 0 },
              }))
            }
          />
          <DataTableColumnsButton table={table} />
        </div>
        <DataTable
          table={table}
          loading={loading}
          onRowClick={(row) => onRowClick(row)}
          bulkActions={
            <>
              <Button
                size="xs"
                variant="primary"
                icon={ResolveIcon}
                onClick={() => console.info('resolve selection')}
              >
                Resolve
              </Button>
              <Button
                size="xs"
                variant="secondary"
                icon={MuteIcon}
                onClick={() => console.info('mute selection')}
              >
                Mute
              </Button>
            </>
          }
        />
      </div>
    </TooltipProvider>
  );
}

const meta = {
  title: 'Components/DataTable',
  component: DataTable,
  parameters: { layout: 'padded' },
} satisfies Meta<typeof DataTable>;

export default meta;
type Story = StoryObj;

/**
 * The whole pattern at once: selection, per-type header filters, external
 * sort, hover actions and pagination, against a simulated server.
 */
export const Issues: Story = {
  render: () => <IssuesTable />,
};

/**
 * The header is the column's control. Opening "Level" offers its values;
 * ticking one narrows the table immediately and rewinds to the first page.
 */
export const FilterFromHeader: Story = {
  render: () => <IssuesTable />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const body = within(document.body);

    await userEvent.click(canvas.getByRole('button', { name: /Level/ }));
    const panel = await body.findByRole('dialog');

    await userEvent.click(
      within(panel).getByRole('button', { name: /Warning/ }),
    );
    await waitFor(() =>
      expect(canvas.queryByText(/PaymentDeclined/)).not.toBeInTheDocument(),
    );
    await expect(
      canvas.getAllByText(/ValidationError|SlowQuery/).length,
    ).toBeGreaterThan(0);

    // The header now reports the filter without being open.
    await userEvent.keyboard('{Escape}');
    await waitFor(() =>
      expect(body.queryByRole('dialog')).not.toBeInTheDocument(),
    );
  },
};

/** Sorting is chosen in the panel, worded in the column's own terms. */
export const SortFromHeader: Story = {
  render: () => <IssuesTable />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const body = within(document.body);

    await userEvent.click(canvas.getByRole('button', { name: /Last seen/ }));
    const panel = await body.findByRole('dialog');
    await userEvent.click(
      within(panel).getByRole('button', { name: /Newest first/ }),
    );

    await waitFor(() => {
      const th = canvasElement.querySelector('th[aria-sort="descending"]');
      expect(th?.textContent).toContain('Last seen');
    });
  },
};

/**
 * Ticking rows turns the header into the bulk strip: count, actions, Clear.
 * Emptying the selection hands the column titles back.
 */
export const Selection: Story = {
  render: () => <IssuesTable />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    await userEvent.click(
      canvas.getByRole('checkbox', { name: 'Select all rows on this page' }),
    );

    const toolbar = canvas.getByRole('toolbar', {
      name: 'Actions for the selected rows',
    });
    await expect(within(toolbar).getByText('10 selected')).toBeInTheDocument();
    await expect(
      within(toolbar).getByRole('button', { name: 'Resolve' }),
    ).toBeInTheDocument();

    // Clear empties the selection and the column titles come back.
    await userEvent.click(
      within(toolbar).getByRole('button', { name: 'Clear' }),
    );
    await waitFor(() =>
      expect(canvas.queryByRole('toolbar')).not.toBeInTheDocument(),
    );
    await expect(
      canvas.getByRole('button', { name: /Events/ }),
    ).toBeInTheDocument();
  },
};

/** Every row ends in the same ⋯ menu; hover reveals nothing it hides. */
export const RowMenu: Story = {
  render: () => <IssuesTable />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const body = within(document.body);

    const menus = canvas.getAllByRole('button', { name: 'Row actions' });
    await expect(menus.length).toBe(10);

    const first = menus[0] as HTMLElement;
    await userEvent.click(first);
    const menu = await body.findByRole('menu');
    await expect(
      within(menu).getByRole('menuitem', { name: /Resolve/ }),
    ).toBeInTheDocument();
    await expect(
      within(menu).getByRole('menuitem', { name: /Delete/ }),
    ).toBeInTheDocument();

    // Escape closes and the trigger takes the focus back.
    await userEvent.keyboard('{Escape}');
    await waitFor(() => expect(first).toHaveFocus());
  },
};

/** A row opens with Enter once focus reaches it -- the keyboard path for `onRowClick`. */
const rowOpensFromKeyboardSpy = fn();

export const RowOpensFromKeyboard: Story = {
  render: () => <IssuesTable onRowClick={rowOpensFromKeyboardSpy} />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const row = canvas.getAllByRole('row')[1] as HTMLElement;

    // Tab through the toolbar and header controls that precede the row --
    // real key events, bounded so an unrelated column change cannot hang it.
    for (let tabs = 0; document.activeElement !== row; tabs++) {
      await expect(tabs).toBeLessThan(60);
      await userEvent.tab();
    }
    await expect(row).toHaveFocus();

    await userEvent.keyboard('{Enter}');
    await expect(rowOpensFromKeyboardSpy).toHaveBeenCalledWith(
      expect.objectContaining({ id: expect.stringMatching(/^RUSTRAK-\d+$/) }),
    );
  },
};

/**
 * One filter state, two views of it: ticking a value in a column's panel
 * grows a chip in the query bar, and removing the chip puts the column's
 * indicator out.
 */
export const BarAndHeaderStayInSync: Story = {
  render: () => <IssuesTable />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const body = within(document.body);

    await userEvent.click(canvas.getByRole('button', { name: /Level/ }));
    const panel = await body.findByRole('dialog');
    await userEvent.click(
      within(panel).getByRole('button', { name: /Warning/ }),
    );
    await userEvent.keyboard('{Escape}');

    // The panel's tick is the bar's chip.
    await expect(canvas.getByText('level:')).toBeInTheDocument();

    // And removing the chip clears the column's filter.
    await userEvent.click(
      canvas.getByRole('button', { name: 'Remove Level filter' }),
    );
    await waitFor(() =>
      expect(
        canvas.queryByRole('button', { name: 'Remove Level filter' }),
      ).not.toBeInTheDocument(),
    );
    await expect(canvas.getAllByText(/PaymentDeclined/).length).toBeGreaterThan(
      0,
    );
  },
};

/** Skeletons only when there is nothing to keep on screen. */
export const Loading: Story = {
  render: () => <IssuesTable loading data={[]} />,
};

/** An empty result names the cause and offers the way back. */
export const Empty: Story = {
  render: function EmptyStory() {
    const [query, setQuery] = useState<DataTableQuery>(() => ({
      ...emptyTableQuery(10),
      search: 'nothing matches this',
    }));
    const table = useDataTable({
      data: [],
      columns,
      rowCount: 0,
      query,
      onQueryChange: (updater) => setQuery((previous) => updater(previous)),
      getRowId: (issue) => issue.id,
    });
    return (
      <TooltipProvider>
        <div className="flex h-105 flex-col">
          <DataTable table={table} />
        </div>
      </TooltipProvider>
    );
  },
};
