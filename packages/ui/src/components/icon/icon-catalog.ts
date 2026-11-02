import { fromHugeicons } from './adapters/hugeicons';
import {
  ArrowDownRight,
  ArrowLeft,
  ArrowLeftRight,
  ArrowRight,
  ArrowUpRight,
  Bell,
  Bookmark,
  BookOpen,
  Bot,
  Braces,
  Bug,
  CalendarClock,
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronsLeft,
  ChevronsRight,
  ChevronsUpDown,
  ChevronUp,
  Circle,
  CircleAlert,
  CircleCheck,
  CircleHelp,
  CircleX,
  Clock,
  Columns3,
  Copy,
  Download,
  Ellipsis,
  EllipsisVertical,
  ExternalLink,
  Eye,
  EyeOff,
  Folder,
  GitBranch,
  GitCommitHorizontal,
  GitMerge,
  Globe,
  GripVertical,
  HardDrive,
  Hash,
  House,
  Inbox,
  KeyRound,
  LayoutDashboard,
  Link,
  ListFilter,
  LoaderCircle,
  LogOut,
  Mail,
  Menu,
  MessageSquare,
  Minus,
  MousePointerClick,
  Navigation,
  Palette,
  PanelLeft,
  PanelRight,
  Paperclip,
  Pencil,
  Play,
  Plug,
  Plus,
  RefreshCw,
  Rocket,
  ScrollText,
  Search,
  Server,
  Settings,
  ShieldAlert,
  SlidersHorizontal,
  Sparkles,
  Star,
  Terminal,
  Trash2,
  TriangleAlert,
  Undo2,
  Unlink,
  User,
  Users,
  VolumeX,
  Webhook,
  Wrench,
  X,
  Zap,
} from './adapters/hugeicons-symbols';
import type { IconComponent } from './icon';

/**
 * The system's icon catalogue.
 *
 * Components and applications import from here, never from the library. The
 * name says what the icon *means* in Rustrak -- `issue`, `release`, `mute`,
 * `overflow` -- not what it draws, so changing the glyph does not force anyone
 * who uses it to change anything.
 *
 * The set is deliberately small. Every icon the product draws is here and
 * nothing else is: a catalogue that grows by accident is how a product ends up
 * with
 * three different check marks.
 */

/* --- Navigation · the sidebar's destinations ----------------------------- */

/** A project's front page. The house, because it is where you land. */
export const OverviewIcon = fromHugeicons('OverviewIcon', House);
/** Every project at once: the workspace, above any one of them. */
export const ProjectsIcon = fromHugeicons('ProjectsIcon', LayoutDashboard);
export const IssuesIcon = fromHugeicons('IssuesIcon', CircleAlert);
export const ReleasesIcon = fromHugeicons('ReleasesIcon', Rocket);
export const PerformanceIcon = fromHugeicons('PerformanceIcon', Zap);
export const AgentsIcon = fromHugeicons('AgentsIcon', Bot);
export const LogsIcon = fromHugeicons('LogsIcon', ScrollText);
export const CronsIcon = fromHugeicons('CronsIcon', CalendarClock);
export const FeedbackIcon = fromHugeicons('FeedbackIcon', MessageSquare);
export const SettingsIcon = fromHugeicons('SettingsIcon', Settings);
export const DocsIcon = fromHugeicons('DocsIcon', BookOpen);
export const SignOutIcon = fromHugeicons('SignOutIcon', LogOut);

/* --- Directions ----------------------------------------------------------- */

export const ChevronRightIcon = fromHugeicons('ChevronRightIcon', ChevronRight);
export const ChevronLeftIcon = fromHugeicons('ChevronLeftIcon', ChevronLeft);
export const ChevronDownIcon = fromHugeicons('ChevronDownIcon', ChevronDown);
export const ChevronUpIcon = fromHugeicons('ChevronUpIcon', ChevronUp);
/** The two-arrow chevron of a thing you can switch: the project card. */
export const SelectorIcon = fromHugeicons('SelectorIcon', ChevronsUpDown);
export const CollapsePanelIcon = fromHugeicons(
  'CollapsePanelIcon',
  ChevronsLeft,
);
export const ExpandPanelIcon = fromHugeicons('ExpandPanelIcon', ChevronsRight);
export const PanelLeftIcon = fromHugeicons('PanelLeftIcon', PanelLeft);
export const PanelRightIcon = fromHugeicons('PanelRightIcon', PanelRight);
/** Out of where you are, up one level: from a project to all of them. */
export const BackIcon = fromHugeicons('BackIcon', ArrowLeft);
export const ArrowRightIcon = fromHugeicons('ArrowRightIcon', ArrowRight);
/** A figure that went up. In this product that is usually bad news. */
export const TrendUpIcon = fromHugeicons('TrendUpIcon', ArrowUpRight);
export const TrendDownIcon = fromHugeicons('TrendDownIcon', ArrowDownRight);
export const CompareIcon = fromHugeicons('CompareIcon', ArrowLeftRight);

/* --- Actions -------------------------------------------------------------- */

export const SearchIcon = fromHugeicons('SearchIcon', Search);
export const FilterIcon = fromHugeicons('FilterIcon', ListFilter);
export const FacetsIcon = fromHugeicons('FacetsIcon', SlidersHorizontal);
export const ColumnsIcon = fromHugeicons('ColumnsIcon', Columns3);
export const NewIcon = fromHugeicons('NewIcon', Plus);
export const RemoveIcon = fromHugeicons('RemoveIcon', Minus);
export const CloseIcon = fromHugeicons('CloseIcon', X);
export const CopyIcon = fromHugeicons('CopyIcon', Copy);
/** Send a test: an alert channel's "try it". */
export const TestIcon = fromHugeicons('TestIcon', Play);
/** Take a value over by hand: a slug that stops following its name. */
export const EditIcon = fromHugeicons('EditIcon', Pencil);
export const ExportIcon = fromHugeicons('ExportIcon', Download);
export const RefreshIcon = fromHugeicons('RefreshIcon', RefreshCw);
export const DeleteIcon = fromHugeicons('DeleteIcon', Trash2);
export const OverflowIcon = fromHugeicons('OverflowIcon', EllipsisVertical);
/** The horizontal ellipsis, for a row's inline menu. */
export const MoreIcon = fromHugeicons('MoreIcon', Ellipsis);
export const MenuIcon = fromHugeicons('MenuIcon', Menu);
/** The handle a row is dragged by to reorder it. */
export const DragIcon = fromHugeicons('DragIcon', GripVertical);
export const AppearanceIcon = fromHugeicons('AppearanceIcon', Palette);
export const ExternalLinkIcon = fromHugeicons('ExternalLinkIcon', ExternalLink);
export const LinkIcon = fromHugeicons('LinkIcon', Link);
export const AttachmentIcon = fromHugeicons('AttachmentIcon', Paperclip);
export const AssignIcon = fromHugeicons('AssignIcon', Users);
export const SaveViewIcon = fromHugeicons('SaveViewIcon', Bookmark);
export const StarIcon = fromHugeicons('StarIcon', Star);
export const CommentIcon = fromHugeicons('CommentIcon', MessageSquare);
export const AutofixIcon = fromHugeicons('AutofixIcon', Sparkles);
export const ConfigureIcon = fromHugeicons('ConfigureIcon', Wrench);
/** The tick beside the option that is in effect: the current project. */
export const SelectedIcon = fromHugeicons('SelectedIcon', Check);

/* --- Issue lifecycle ------------------------------------------------------
   The three verbs an issue understands. `resolve` is a plain check because it
   is the primary action on the detail screen and shares the button with the
   label; `mute` is the crossed speaker, which is what silencing looks like
   everywhere else. */

export const ResolveIcon = fromHugeicons('ResolveIcon', Check);
export const MuteIcon = fromHugeicons('MuteIcon', VolumeX);
export const UnresolveIcon = fromHugeicons('UnresolveIcon', CircleX);
/** Take it back: the action an undo toast carries. */
export const UndoIcon = fromHugeicons('UndoIcon', Undo2);

/* --- Severity and status --------------------------------------------------
   `error` is the crossed circle, `warning` the triangle, `ok` the ticked
   circle. Shape, not only colour: severity has to survive being printed in
   grey. */

export const ErrorIcon = fromHugeicons('ErrorIcon', CircleX);
export const WarningIcon = fromHugeicons('WarningIcon', TriangleAlert);
export const OkIcon = fromHugeicons('OkIcon', CircleCheck);
export const InfoIcon = fromHugeicons('InfoIcon', CircleAlert);
/** "How does this work?": opens an explanation, never a setting. */
export const HelpIcon = fromHugeicons('HelpIcon', CircleHelp);
export const SpinnerIcon = fromHugeicons('SpinnerIcon', LoaderCircle);
export const NotificationIcon = fromHugeicons('NotificationIcon', Bell);
export const TimeIcon = fromHugeicons('TimeIcon', Clock);
export const EmptyIcon = fromHugeicons('EmptyIcon', Inbox);
export const WatchIcon = fromHugeicons('WatchIcon', Eye);
/** A secret field's reveal toggle: what shows the value, and what hides it. */
export const RevealIcon = fromHugeicons('RevealIcon', Eye);
export const ConcealIcon = fromHugeicons('ConcealIcon', EyeOff);

/* --- Entities -------------------------------------------------------------
   What the data is about, rather than which page shows it. */

export const ReleaseIcon = fromHugeicons('ReleaseIcon', GitBranch);
export const CommitIcon = fromHugeicons('CommitIcon', GitCommitHorizontal);
export const MergeIcon = fromHugeicons('MergeIcon', GitMerge);
export const EnvironmentIcon = fromHugeicons('EnvironmentIcon', Server);
export const MemberIcon = fromHugeicons('MemberIcon', User);
export const TeamIcon = fromHugeicons('TeamIcon', Users);
export const IntegrationsIcon = fromHugeicons('IntegrationsIcon', Plug);
export const StorageIcon = fromHugeicons('StorageIcon', HardDrive);
/** An API token: what lets something other than a person in. */
export const TokensIcon = fromHugeicons('TokensIcon', KeyRound);
export const PayloadIcon = fromHugeicons('PayloadIcon', Braces);
export const HashIcon = fromHugeicons('HashIcon', Hash);
export const MailIcon = fromHugeicons('MailIcon', Mail);
export const WebhookIcon = fromHugeicons('WebhookIcon', Webhook);
export const UnlinkIcon = fromHugeicons('UnlinkIcon', Unlink);
export const SecurityIcon = fromHugeicons('SecurityIcon', ShieldAlert);
export const FirstPageIcon = fromHugeicons('FirstPageIcon', ChevronsLeft);
export const LastPageIcon = fromHugeicons('LastPageIcon', ChevronsRight);
export const BugIcon = fromHugeicons('BugIcon', Bug);
export const FolderIcon = fromHugeicons('FolderIcon', Folder);
export const DotIcon = fromHugeicons('DotIcon', Circle);
export const NetworkIcon = fromHugeicons('NetworkIcon', Globe);
export const ClickIcon = fromHugeicons('ClickIcon', MousePointerClick);
export const NavigationIcon = fromHugeicons('NavigationIcon', Navigation);
export const ConsoleIcon = fromHugeicons('ConsoleIcon', Terminal);

/**
 * The same icons reachable by name, for navigation and action bars that are
 * described with data instead of JSX.
 */
export const icons = {
  overview: OverviewIcon,
  projects: ProjectsIcon,
  crons: CronsIcon,
  feedback: FeedbackIcon,
  docs: DocsIcon,
  signOut: SignOutIcon,
  selected: SelectedIcon,
  issues: IssuesIcon,
  releases: ReleasesIcon,
  performance: PerformanceIcon,
  agents: AgentsIcon,
  logs: LogsIcon,
  settings: SettingsIcon,
  chevronRight: ChevronRightIcon,
  chevronLeft: ChevronLeftIcon,
  chevronDown: ChevronDownIcon,
  chevronUp: ChevronUpIcon,
  selector: SelectorIcon,
  collapsePanel: CollapsePanelIcon,
  expandPanel: ExpandPanelIcon,
  panelLeft: PanelLeftIcon,
  panelRight: PanelRightIcon,
  back: BackIcon,
  arrowRight: ArrowRightIcon,
  trendUp: TrendUpIcon,
  trendDown: TrendDownIcon,
  compare: CompareIcon,
  search: SearchIcon,
  filter: FilterIcon,
  facets: FacetsIcon,
  columns: ColumnsIcon,
  new: NewIcon,
  remove: RemoveIcon,
  close: CloseIcon,
  copy: CopyIcon,
  test: TestIcon,
  edit: EditIcon,
  export: ExportIcon,
  refresh: RefreshIcon,
  delete: DeleteIcon,
  overflow: OverflowIcon,
  more: MoreIcon,
  menu: MenuIcon,
  drag: DragIcon,
  appearance: AppearanceIcon,
  externalLink: ExternalLinkIcon,
  link: LinkIcon,
  attachment: AttachmentIcon,
  assign: AssignIcon,
  saveView: SaveViewIcon,
  star: StarIcon,
  comment: CommentIcon,
  autofix: AutofixIcon,
  configure: ConfigureIcon,
  resolve: ResolveIcon,
  mute: MuteIcon,
  unresolve: UnresolveIcon,
  undo: UndoIcon,
  error: ErrorIcon,
  warning: WarningIcon,
  ok: OkIcon,
  info: InfoIcon,
  help: HelpIcon,
  spinner: SpinnerIcon,
  notification: NotificationIcon,
  time: TimeIcon,
  empty: EmptyIcon,
  watch: WatchIcon,
  release: ReleaseIcon,
  commit: CommitIcon,
  merge: MergeIcon,
  environment: EnvironmentIcon,
  member: MemberIcon,
  team: TeamIcon,
  integrations: IntegrationsIcon,
  storage: StorageIcon,
  tokens: TokensIcon,
  payload: PayloadIcon,
  hash: HashIcon,
  mail: MailIcon,
  webhook: WebhookIcon,
  unlink: UnlinkIcon,
  security: SecurityIcon,
  firstPage: FirstPageIcon,
  lastPage: LastPageIcon,
  bug: BugIcon,
  folder: FolderIcon,
  dot: DotIcon,
  network: NetworkIcon,
  click: ClickIcon,
  navigation: NavigationIcon,
  console: ConsoleIcon,
} as const satisfies Record<string, IconComponent>;

export type IconName = keyof typeof icons;
