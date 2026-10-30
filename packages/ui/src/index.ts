/**
 * @rustrak/ui -- the design system behind the Rustrak dashboard.
 *
 * Everything is exported from here. There are no deep imports: the paths inside
 * `src` are an implementation detail and moving a file should not break a
 * consumer.
 */

/* --- Foundations --------------------------------------------------------- */

export { cn } from './lib/cn';
export {
  focusRing,
  focusRingInset,
  focusRingWithin,
} from './lib/focus';
export {
  chevronFlip,
  collapseFade,
  dropIn,
  interactiveTransition,
  popTransition,
  pressNudge,
  pressScale,
  pressScaleSmall,
  pressScaleTrigger,
  scopeEnter,
  scopeGroup,
  slideTransition,
  swapAnimation,
  wipeReveal,
} from './lib/motion';
export type {
  ColorToken,
  DurationToken,
  EaseToken,
  FontToken,
  RadiusToken,
  ShadowToken,
  SpacingToken,
  TextToken,
} from './lib/tokens';
export {
  colorTokens,
  durationTokens,
  easeTokens,
  fontTokens,
  radiusTokens,
  shadowTokens,
  spacingTokens,
  textTokens,
} from './lib/tokens';
export type { VariantProps } from './lib/tv';
export { tv } from './lib/tv';
export type { WithClassName } from './lib/types';
export { useMobileBreakpoint } from './lib/use-mobile';

/* --- Brand --------------------------------------------------------------- */

export { Wordmark } from './components/brand/wordmark';

/* --- Icons --------------------------------------------------------------- */

export { fromLucide } from './components/icon/adapters/lucide';
export type {
  IconComponent,
  IconProps,
  IconSize,
} from './components/icon/icon';
export { iconVariants } from './components/icon/icon';
export * from './components/icon/icon-catalog';

/* --- Presentation -------------------------------------------------------- */

export type { AvatarProps, AvatarShape } from './components/avatar/avatar';
export { Avatar } from './components/avatar/avatar';
export type { CountProps } from './components/count/count';
export { Count } from './components/count/count';
export type { KbdProps } from './components/kbd/kbd';
export { Kbd } from './components/kbd/kbd';
export type { SeparatorProps } from './components/separator/separator';
export { Separator } from './components/separator/separator';
export type { SkeletonProps } from './components/skeleton/skeleton';
export { Skeleton } from './components/skeleton/skeleton';
export type { SpinnerProps } from './components/spinner/spinner';
export { Spinner } from './components/spinner/spinner';
export type { TagProps, TagTone } from './components/tag/tag';
export { Tag } from './components/tag/tag';
export type { TextProps, TextTone, TextVariant } from './components/text/text';
export { Text } from './components/text/text';

/* --- Actions ------------------------------------------------------------- */

export type {
  ButtonProps,
  ButtonSize,
  ButtonVariant,
} from './components/button/button';
export { Button } from './components/button/button';
export type { SplitButtonProps } from './components/button/split-button';
export { SplitButton } from './components/button/split-button';

/* --- Navigation ---------------------------------------------------------- */

export type {
  BreadcrumbsProps,
  Crumb,
} from './components/breadcrumbs/breadcrumbs';
export { Breadcrumbs } from './components/breadcrumbs/breadcrumbs';
export type { MenuProps } from './components/menu/menu';
export {
  Menu,
  MenuActions,
  MenuGroup,
  MenuGroupLabel,
} from './components/menu/menu';
export type { MenuAction } from './components/menu/menu-parts';
export { explainAction } from './components/menu/menu-parts';
export type {
  SegmentedControlProps,
  SegmentedItemProps,
} from './components/segmented-control/segmented-control';
export {
  SegmentedControl,
  SegmentedItem,
} from './components/segmented-control/segmented-control';
export type {
  TabListProps,
  TabPanelProps,
  TabProps,
  TabsProps,
  TabsSize,
} from './components/tabs/tabs';
export { Tab, TabList, TabPanel, Tabs } from './components/tabs/tabs';
export type { TooltipProps } from './components/tooltip/tooltip';
export { Tooltip, TooltipProvider } from './components/tooltip/tooltip';

/* --- Forms --------------------------------------------------------------- */

export type { CheckboxProps } from './components/checkbox/checkbox';
export { Checkbox } from './components/checkbox/checkbox';
export type {
  FieldErrorProps,
  FieldHintProps,
  FieldLabelProps,
  FieldProps,
} from './components/field/field';
export {
  Field,
  FieldError,
  FieldHint,
  FieldLabel,
} from './components/field/field';
export type {
  InputActionProps,
  InputProps,
  TextareaProps,
} from './components/input/input';
export { Input, InputAction, Textarea } from './components/input/input';
export type { InputShellSize } from './components/input/input-shell';
export { inputShell } from './components/input/input-shell';
export type { PopoverProps } from './components/popover/popover';
export { Popover } from './components/popover/popover';
export type { SwitchProps } from './components/switch/switch';
export { Switch } from './components/switch/switch';

/* --- Data table ---------------------------------------------------------- */

export type { DataTableColumnsButtonProps } from './components/data-table/columns-menu';
export { DataTableColumnsButton } from './components/data-table/columns-menu';
export type {
  DataTableEmptyProps,
  DataTableProps,
} from './components/data-table/data-table';
export { DataTable } from './components/data-table/data-table';
export type {
  ColumnFilterSpec,
  DataTableColumnDef,
  DataTableColumnMeta,
  DataTableFeatures,
  FilterOption,
} from './components/data-table/features';
export {
  createDataTableColumnHelper,
  dataTableFeatures,
} from './components/data-table/features';
export type { DataTableLabels } from './components/data-table/labels';
export {
  DataTableLabelsProvider,
  useDataTableLabels,
} from './components/data-table/labels';
export type { DataTablePaginationProps } from './components/data-table/pagination';
export { DataTablePagination } from './components/data-table/pagination';
export type {
  DataTableQuery,
  FilterVariants,
} from './components/data-table/query';
export {
  DEFAULT_PAGE_SIZE,
  emptyTableQuery,
  formatFilterQuery,
  parseFilterQuery,
  parseTableQuery,
  serializeTableQuery,
} from './components/data-table/query';
export type {
  DataTableInstance,
  UseDataTableOptions,
} from './components/data-table/use-data-table';
export { useDataTable } from './components/data-table/use-data-table';

/* --- Charts -------------------------------------------------------------- */

export type { BarsChartProps } from './components/chart/bars-chart';
export { BarsChart } from './components/chart/bars-chart';
export type { ChartTooltipProps } from './components/chart/chart-parts';
export { ChartLegend, ChartTooltip } from './components/chart/chart-parts';
export type { ChartSeries } from './components/chart/chart-series';
export { seriesColor } from './components/chart/chart-series';
export type {
  SparklineProps,
  SparklineTone,
} from './components/chart/sparkline';
export { Sparkline } from './components/chart/sparkline';
export type { TimeSeriesChartProps } from './components/chart/time-series-chart';
export { TimeSeriesChart } from './components/chart/time-series-chart';

/* --- Waterfall ------------------------------------------------------------ */

export { formatSpanDuration } from './components/waterfall/format';
export type {
  WaterfallProps,
  WaterfallSpan,
} from './components/waterfall/waterfall';
export { Waterfall } from './components/waterfall/waterfall';

/* --- Overlays ------------------------------------------------------------ */

export type {
  AlertOptions,
  ConfirmOptions,
} from './components/dialog/confirm';
export { alert, confirm } from './components/dialog/confirm';
export type {
  DialogBodyProps,
  DialogFooterProps,
  DialogHeaderProps,
  DialogProps,
  DialogSize,
  DialogTone,
} from './components/dialog/dialog';
export {
  Dialog,
  DialogBody,
  DialogClose,
  DialogFooter,
  DialogHeader,
} from './components/dialog/dialog';
export type { DialogProviderProps } from './components/dialog/dialog-manager';
export { DialogProvider } from './components/dialog/dialog-manager';
export type {
  DialogComponent,
  DialogDefinition,
  DialogDefinitionOptions,
  DialogHostProps,
} from './components/dialog/dialog-store';
export {
  closeAllDialogs,
  createDialog,
} from './components/dialog/dialog-store';
export type {
  ToastActionSpec,
  ToastOptions,
  ToastPromiseState,
  ToastProviderProps,
  ToastTone,
  UseToastReturn,
} from './components/toast/toast';
export { ToastProvider, useToast } from './components/toast/toast';

/* --- Query bar ----------------------------------------------------------- */

export type { QueryBarProps } from './components/query-bar/query-bar';
export { QueryBar } from './components/query-bar/query-bar';
export type { QueryField } from './components/query-bar/query-bar-parts';
export {
  queryFieldsFromColumns,
  variantsFromFields,
} from './components/query-bar/query-bar-parts';

/* --- Shell --------------------------------------------------------------- */

export type {
  AppShellProps,
  PageHeaderProps,
  PageProps,
} from './components/shell/app-shell';
export {
  AppShell,
  Page,
  PageHeader,
} from './components/shell/app-shell';
export type {
  SidebarBrandProps,
  SidebarGroupProps,
  SidebarItemProps,
  SidebarProjectProps,
  SidebarProps,
  SidebarScopeProps,
  SidebarSearchProps,
  SidebarSubmenuProps,
  SidebarUserProps,
} from './components/shell/sidebar';
export {
  Sidebar,
  SidebarBrand,
  SidebarGroup,
  SidebarItem,
  SidebarProject,
  SidebarScope,
  SidebarSearch,
  SidebarSubmenu,
  SidebarUser,
} from './components/shell/sidebar';
export type { SidebarProviderProps } from './components/shell/sidebar-context';
export {
  SidebarProvider,
  useSidebar,
} from './components/shell/sidebar-context';
export type {
  TopbarActionProps,
  TopbarProps,
} from './components/shell/topbar';
export {
  Topbar,
  TopbarAction,
  TopbarMenuButton,
} from './components/shell/topbar';
