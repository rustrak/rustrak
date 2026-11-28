//! The command tree.
//!
//! Global flags (`--url`, `--token`, `--profile`, `--project`, `--json`) come
//! before or after the subcommand, because clap's `global = true` puts them on
//! every one. Everything else is grouped the way the dashboard's sidebar is:
//! issues, events, logs, traces, performance, releases, cron monitors, alerts,
//! then the instance-level commands.

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "rustrak",
    version,
    about = "Command-line client for Rustrak, self-hosted error tracking",
    long_about = "Triage issues, read logs and traces, and manage projects on a Rustrak \
                  instance from a terminal.\n\nRun `rustrak auth login` once; every later \
                  command uses the stored token.",
    propagate_version = true,
    disable_help_subcommand = true,
    arg_required_else_help = true
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Command,
}

/// The flags every command accepts.
#[derive(Debug, Args)]
pub struct GlobalArgs {
    /// The instance to talk to; overrides the profile's URL.
    #[arg(long, global = true, env = "RUSTRAK_URL", value_name = "URL")]
    pub url: Option<String>,

    /// The API token; overrides the profile's token.
    #[arg(long, global = true, env = "RUSTRAK_TOKEN", value_name = "TOKEN")]
    pub token: Option<String>,

    /// The profile to use; overrides the current one.
    #[arg(
        long,
        global = true,
        short = 'p',
        env = "RUSTRAK_PROFILE",
        value_name = "NAME"
    )]
    pub profile: Option<String>,

    /// The project id or slug the command acts on.
    #[arg(long, global = true, env = "RUSTRAK_PROJECT", value_name = "ID|SLUG")]
    pub project: Option<String>,

    /// Machine-readable JSON, whatever the destination.
    #[arg(long, global = true, conflicts_with = "table")]
    pub json: bool,

    /// A table, even when stdout is a pipe.
    #[arg(long, global = true, conflicts_with = "json")]
    pub table: bool,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Sign in, sign out, and inspect the current identity.
    #[command(subcommand)]
    Auth(AuthCommand),

    /// Read and write the local config file.
    #[command(subcommand)]
    Config(ConfigCommand),

    /// Projects: list, inspect, create, delete, and read their health.
    #[command(subcommand)]
    Project(ProjectCommand),

    /// Issues: triage, assign, comment, and read their events.
    #[command(subcommand)]
    Issue(IssueCommand),

    /// One event, by id, or an issue's latest.
    #[command(subcommand)]
    Event(EventCommand),

    /// Structured logs.
    #[command(subcommand)]
    Log(LogCommand),

    /// AI agent runs: traces, models and tools.
    #[command(subcommand)]
    Trace(TraceCommand),

    /// Performance: transactions and their latency.
    #[command(subcommand)]
    Perf(PerfCommand),

    /// Releases and their health.
    #[command(subcommand)]
    Release(ReleaseCommand),

    /// Cron monitors.
    #[command(subcommand)]
    Monitor(MonitorCommand),

    /// Alert rules.
    #[command(subcommand)]
    Alert(AlertCommand),

    /// The instance's users.
    #[command(subcommand)]
    Team(TeamCommand),

    /// API tokens.
    #[command(subcommand)]
    Token(TokenCommand),

    /// Send a test event to a project's DSN, the way an SDK would.
    Send(SendArgs),

    /// Call any endpoint directly, for what a command does not wrap yet.
    Api(ApiArgs),

    /// Print the instance's version.
    Version,

    /// Check that the instance answers.
    Health,

    /// Generate a shell completion script.
    Completion(CompletionArgs),
}

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Sign in with an email and password, and store a token.
    Login(LoginArgs),
    /// Report the profile, instance, and the identity behind the token.
    Status,
    /// Show the signed-in user.
    Whoami,
    /// Remove the stored token for a profile.
    Logout(LogoutArgs),
}

#[derive(Debug, Args)]
pub struct LoginArgs {
    /// The instance to sign in to. Required the first time.
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,

    /// The account's email. Prompted when omitted.
    #[arg(long, value_name = "EMAIL")]
    pub email: Option<String>,

    /// The account's password. Prompted without echo when omitted, which is
    /// the safe default: a password in `ps` is a password leaked.
    #[arg(long, value_name = "PASSWORD")]
    pub password: Option<String>,

    /// The profile to store it in.
    #[arg(long, value_name = "NAME", default_value = "default")]
    pub profile_name: String,

    /// A description for the minted token.
    #[arg(long, value_name = "TEXT")]
    pub description: Option<String>,

    /// Reuse this token instead of signing in. For an instance whose password
    /// login is disabled, or a token minted in the dashboard.
    #[arg(long, value_name = "TOKEN", conflicts_with_all = ["email", "password"])]
    pub token: Option<String>,
}

#[derive(Debug, Args)]
pub struct LogoutArgs {
    /// The profile to sign out of.
    #[arg(long, value_name = "NAME")]
    pub profile_name: Option<String>,

    /// Also revoke the token on the server, so it cannot be reused.
    #[arg(long)]
    pub revoke: bool,

    /// Do not ask before revoking.
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// List the profiles.
    List,
    /// Show one profile, with the token masked.
    Show(ConfigShowArgs),
    /// Make a profile the default.
    Use(ConfigUseArgs),
    /// Print the config file's path.
    Path,
    /// Set a profile's URL or token.
    Set(ConfigSetArgs),
    /// Delete a profile.
    Remove(ConfigRemoveArgs),
}

#[derive(Debug, Args)]
pub struct ConfigShowArgs {
    pub name: Option<String>,
}

#[derive(Debug, Args)]
pub struct ConfigUseArgs {
    pub name: String,
}

#[derive(Debug, Args)]
pub struct ConfigSetArgs {
    /// The profile to edit.
    #[arg(long, value_name = "NAME")]
    pub profile_name: Option<String>,

    /// The instance's URL.
    #[arg(long, value_name = "URL")]
    pub url: Option<String>,

    /// The API token.
    #[arg(long, value_name = "TOKEN")]
    pub token: Option<String>,
}

#[derive(Debug, Args)]
pub struct ConfigRemoveArgs {
    pub name: String,
}

#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    /// List projects.
    List(ProjectListArgs),
    /// Show one project, including its DSN.
    Get(ProjectGetArgs),
    /// Create a project.
    Create(ProjectCreateArgs),
    /// Update a project's name or platform.
    Update(ProjectUpdateArgs),
    /// Delete a project and everything in it.
    Delete(ProjectDeleteArgs),
    /// The errors, new issues and open issues over a window.
    Stats(ProjectStatsArgs),
    /// Crash-free sessions and users.
    Sessions(ProjectStatsArgs),
    /// The project's environments.
    Envs,
    /// Show the DSN to point an SDK at.
    Dsn(ProjectGetArgs),
    /// Make a project the default for later commands.
    Use(ProjectUseArgs),
}

#[derive(Debug, Args)]
pub struct ProjectListArgs {
    /// Free-text filter on the name.
    #[arg(long, value_name = "TEXT")]
    pub query: Option<String>,

    /// How many rows.
    #[arg(long, default_value_t = 20, value_name = "N")]
    pub limit: i64,

    /// The page to read.
    #[arg(long, default_value_t = 1, value_name = "N")]
    pub page: i64,

    /// The window the per-project counters cover: `24h`, `7d`, `14d`.
    #[arg(long, value_name = "PERIOD")]
    pub period: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProjectGetArgs {
    /// The project id or slug; defaults to `--project`.
    ///
    /// `id = "project_ref"` because a positional called `project` collides
    /// with the global `--project` flag: clap keys both on the same id and
    /// refuses the pair at parse time.
    #[arg(id = "project_ref")]
    pub project_ref: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProjectCreateArgs {
    /// The project's name.
    pub name: String,

    /// A platform identifier, such as `python-django` or `javascript-nextjs`.
    #[arg(long, value_name = "PLATFORM")]
    pub platform: Option<String>,

    /// A slug; derived from the name when omitted.
    #[arg(long, value_name = "SLUG")]
    pub slug: Option<String>,

    /// Make it the default project.
    #[arg(long)]
    pub r#use: bool,
}

#[derive(Debug, Args)]
pub struct ProjectUpdateArgs {
    #[arg(id = "project_ref")]
    pub project_ref: Option<String>,

    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,

    #[arg(long, value_name = "PLATFORM")]
    pub platform: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProjectDeleteArgs {
    #[arg(id = "project_ref")]
    pub project_ref: Option<String>,

    /// Do not ask before deleting.
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct ProjectStatsArgs {
    #[arg(id = "project_ref")]
    pub project_ref: Option<String>,

    /// The window: `24h`, `7d`, `30d`.
    #[arg(long, value_name = "PERIOD")]
    pub period: Option<String>,

    #[arg(long, value_name = "ENV")]
    pub environment: Option<String>,
}

#[derive(Debug, Args)]
pub struct ProjectUseArgs {
    pub project: String,
}

#[derive(Debug, Subcommand)]
pub enum IssueCommand {
    /// List issues, newest or most frequent first.
    List(IssueListArgs),
    /// Show one issue in full.
    Get(IssueGetArgs),
    /// Resolve issues.
    Resolve(IssueStatusArgs),
    /// Reopen issues.
    Unresolve(IssueStatusArgs),
    /// Ignore issues.
    Ignore(IssueStatusArgs),
    /// Set the priority of issues.
    Priority(IssuePriorityArgs),
    /// The events that make up an issue.
    Events(IssueEventsArgs),
    /// One event of the issue, by id or `latest`.
    Event(IssueEventArgs),
    /// The issue's activity log: status changes and comments.
    Activity(IssueGetArgs),
    /// Comment on an issue.
    Comment(IssueCommentArgs),
    /// Mark an issue as seen by you.
    Seen(IssueGetArgs),
    /// The issue's tag values.
    Tags(IssueGetArgs),
    /// Delete issues.
    Delete(IssueDeleteArgs),
    /// The issue's event-count timeseries.
    Stats(IssueStatsArgs),
}

#[derive(Debug, Clone, ValueEnum)]
pub enum FilterArg {
    /// Open (unresolved and unmuted).
    Open,
    /// Resolved.
    Resolved,
    /// Ignored.
    Muted,
    /// Every status.
    All,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum SortArg {
    /// Most recently seen first.
    LastSeen,
    /// Most events first.
    EventCount,
    /// Newest first.
    FirstSeen,
    /// High priority first.
    Priority,
    /// The stable digest order.
    DigestOrder,
}

#[derive(Debug, Args)]
pub struct IssueListArgs {
    /// Free text to match against the title.
    #[arg(long, value_name = "TEXT")]
    pub query: Option<String>,

    #[arg(long, value_enum, default_value_t = FilterArg::Open)]
    pub filter: FilterArg,

    /// Only this level: `fatal`, `error`, `warning`, `info`, `debug`.
    #[arg(long, value_name = "LEVEL")]
    pub level: Option<String>,

    /// Only this priority: `high`, `medium`, `low`.
    #[arg(long, value_name = "PRIORITY")]
    pub priority: Option<String>,

    #[arg(long, value_name = "ENV")]
    pub environment: Option<String>,

    #[arg(long, value_name = "RELEASE")]
    pub release: Option<String>,

    /// Only issues first seen within a window, such as `24h` or `7d`.
    #[arg(long, value_name = "WINDOW")]
    pub first_seen: Option<String>,

    /// Only issues last seen within a window.
    #[arg(long, value_name = "WINDOW")]
    pub last_seen: Option<String>,

    #[arg(long, value_enum, default_value_t = SortArg::LastSeen)]
    pub sort: SortArg,

    /// Ascending instead of descending.
    #[arg(long)]
    pub asc: bool,

    /// How many rows.
    #[arg(long, default_value_t = 25, value_name = "N")]
    pub limit: i64,

    #[arg(long, default_value_t = 1, value_name = "N")]
    pub page: i64,
}

#[derive(Debug, Args)]
pub struct IssueGetArgs {
    /// The issue's id, or its `PROJECT-123` short id.
    pub issue: String,
}

#[derive(Debug, Args)]
pub struct IssueStatusArgs {
    /// Issue ids or `PROJECT-123` short ids.
    #[arg(required = true, value_name = "ISSUE")]
    pub issues: Vec<String>,

    /// Do not ask before changing more than one issue.
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum PriorityArg {
    Low,
    Medium,
    High,
}

#[derive(Debug, Args)]
pub struct IssuePriorityArgs {
    /// `high`, `medium` or `low`.
    #[arg(value_enum)]
    pub priority: PriorityArg,

    /// Issue ids or `PROJECT-123` short ids.
    #[arg(required = true, value_name = "ISSUE")]
    pub issues: Vec<String>,
}

#[derive(Debug, Args)]
pub struct IssueEventsArgs {
    pub issue: String,

    /// How many events.
    #[arg(long, default_value_t = 20, value_name = "N")]
    pub limit: i64,

    /// Oldest first.
    #[arg(long)]
    pub asc: bool,

    #[arg(long, value_name = "ENV")]
    pub environment: Option<String>,

    /// The cursor a previous page returned.
    #[arg(long, value_name = "CURSOR")]
    pub cursor: Option<String>,
}

#[derive(Debug, Args)]
pub struct IssueEventArgs {
    pub issue: String,

    /// An event id, or `latest`.
    #[arg(default_value = "latest", value_name = "EVENT|latest")]
    pub event: String,
}

#[derive(Debug, Args)]
pub struct IssueCommentArgs {
    pub issue: String,

    /// The comment's text.
    pub text: String,
}

#[derive(Debug, Args)]
pub struct IssueDeleteArgs {
    /// Issue ids or `PROJECT-123` short ids.
    #[arg(required = true, value_name = "ISSUE")]
    pub issues: Vec<String>,

    /// Do not ask before deleting.
    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct IssueStatsArgs {
    pub issue: String,

    /// `24h` or `30d`.
    #[arg(long, default_value = "24h", value_name = "WINDOW")]
    pub window: String,

    /// Draw the series as a sparkline.
    #[arg(long)]
    pub spark: bool,
}

#[derive(Debug, Subcommand)]
pub enum EventCommand {
    /// One event, in full: stack frames, tags, and the raw payload.
    Get(EventGetArgs),
}

#[derive(Debug, Args)]
pub struct EventGetArgs {
    /// The event id, in hyphenated or bare hex form.
    pub event: String,

    /// Show only the raw Sentry payload.
    #[arg(long)]
    pub raw: bool,
}

#[derive(Debug, Subcommand)]
pub enum LogCommand {
    /// List logs, newest first.
    List(LogListArgs),
    /// Follow the newest logs, printing each as it arrives.
    Tail(LogTailArgs),
    /// One log entry, in full, including its attributes.
    Get(LogGetArgs),
}

#[derive(Debug, Args)]
pub struct LogListArgs {
    /// Only this severity.
    #[arg(long, value_name = "LEVEL")]
    pub level: Option<String>,

    /// Only logs carrying this trace id.
    #[arg(long, value_name = "TRACE_ID")]
    pub trace_id: Option<String>,

    #[arg(long, value_name = "ENV")]
    pub environment: Option<String>,

    /// A substring to match in the body.
    #[arg(long, value_name = "TEXT")]
    pub query: Option<String>,

    /// The window: `1h`, `24h`, `7d`.
    #[arg(long, value_name = "PERIOD")]
    pub period: Option<String>,

    #[arg(long, default_value_t = 50, value_name = "N")]
    pub limit: i64,

    #[arg(long, default_value_t = 1, value_name = "N")]
    pub page: i64,
}

#[derive(Debug, Args)]
pub struct LogTailArgs {
    #[arg(long, value_name = "LEVEL")]
    pub level: Option<String>,

    #[arg(long, value_name = "ENV")]
    pub environment: Option<String>,

    #[arg(long, value_name = "TEXT")]
    pub query: Option<String>,

    /// The window the first poll reads.
    #[arg(long, default_value = "15m", value_name = "PERIOD")]
    pub period: String,

    /// Seconds between polls.
    #[arg(long, default_value_t = 3, value_name = "SECS")]
    pub interval: u64,
}

#[derive(Debug, Args)]
pub struct LogGetArgs {
    /// The log's id.
    pub log: String,
}

#[derive(Debug, Subcommand)]
pub enum TraceCommand {
    /// Headline numbers for agent runs.
    Summary(AgentArgs),
    /// One row per trace.
    List(AgentTraceArgs),
    /// Calls by model.
    Models(AgentArgs),
    /// Calls by tool.
    Tools(AgentArgs),
}

#[derive(Debug, Args)]
pub struct AgentArgs {
    /// The window, in hours.
    #[arg(long, value_name = "HOURS")]
    pub hours: Option<i64>,

    #[arg(long, value_name = "ENV")]
    pub environment: Option<String>,
}

#[derive(Debug, Args)]
pub struct AgentTraceArgs {
    #[arg(long, value_name = "HOURS")]
    pub hours: Option<i64>,

    #[arg(long, value_name = "ENV")]
    pub environment: Option<String>,

    /// Only traces from this agent.
    #[arg(long, value_name = "NAME")]
    pub agent: Option<String>,

    /// Only traces that called this model.
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,

    /// Only traces with errors.
    #[arg(long)]
    pub errors: bool,

    #[arg(long, default_value_t = 25, value_name = "N")]
    pub limit: i64,

    #[arg(long, default_value_t = 1, value_name = "N")]
    pub page: i64,
}

#[derive(Debug, Subcommand)]
pub enum PerfCommand {
    /// The slowest transactions over a window.
    List(PerfListArgs),
    /// p50/p95/p99 per transaction name, ranked by time spent.
    Stats(PerfStatsArgs),
}

#[derive(Debug, Args)]
pub struct PerfListArgs {
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,

    #[arg(long, value_name = "OP")]
    pub op: Option<String>,

    #[arg(long, value_name = "ENV")]
    pub environment: Option<String>,

    #[arg(long, value_name = "PERIOD")]
    pub period: Option<String>,

    #[arg(long, default_value_t = 25, value_name = "N")]
    pub limit: i64,

    #[arg(long, default_value_t = 1, value_name = "N")]
    pub page: i64,
}

#[derive(Debug, Args)]
pub struct PerfStatsArgs {
    #[arg(long, value_name = "ENV")]
    pub environment: Option<String>,

    #[arg(long, value_name = "PERIOD")]
    pub period: Option<String>,

    #[arg(long, default_value_t = 25, value_name = "N")]
    pub limit: i64,
}

#[derive(Debug, Subcommand)]
pub enum ReleaseCommand {
    /// List releases with their health.
    List(ReleaseListArgs),
    /// One release.
    Get(ReleaseGetArgs),
    /// Register a release.
    Create(ReleaseCreateArgs),
    /// Delete a release.
    Delete(ReleaseDeleteArgs),
}

#[derive(Debug, Args)]
pub struct ReleaseListArgs {
    #[arg(long, value_name = "ENV")]
    pub environment: Option<String>,

    #[arg(long, value_name = "PERIOD")]
    pub period: Option<String>,

    #[arg(long, value_name = "TEXT")]
    pub query: Option<String>,

    #[arg(long, default_value_t = 20, value_name = "N")]
    pub limit: i64,

    #[arg(long, default_value_t = 1, value_name = "N")]
    pub page: i64,
}

#[derive(Debug, Args)]
pub struct ReleaseGetArgs {
    /// `id = "release_version"`: a positional called `version` claims the same
    /// clap id as the auto-generated `--version` flag, and clap refuses that.
    #[arg(id = "release_version")]
    pub release_version: String,
}

#[derive(Debug, Args)]
pub struct ReleaseCreateArgs {
    #[arg(id = "release_version")]
    pub release_version: String,

    #[arg(long, value_name = "REF")]
    pub r#ref: Option<String>,

    #[arg(long, value_name = "URL")]
    pub url: Option<String>,
}

#[derive(Debug, Args)]
pub struct ReleaseDeleteArgs {
    #[arg(id = "release_version")]
    pub release_version: String,

    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Debug, Subcommand)]
pub enum MonitorCommand {
    /// List cron monitors.
    List,
    /// One monitor.
    Get(MonitorGetArgs),
    /// A monitor's recent check-ins.
    Checkins(MonitorCheckinsArgs),
    /// Mute or unmute a monitor.
    Mute(MonitorMuteArgs),
}

#[derive(Debug, Args)]
pub struct MonitorGetArgs {
    pub slug: String,
}

#[derive(Debug, Args)]
pub struct MonitorCheckinsArgs {
    pub slug: String,

    #[arg(long, default_value_t = 20, value_name = "N")]
    pub limit: i64,

    #[arg(long, value_name = "ENV")]
    pub environment: Option<String>,
}

#[derive(Debug, Args)]
pub struct MonitorMuteArgs {
    pub slug: String,

    /// Unmute instead of muting.
    #[arg(long)]
    pub off: bool,
}

#[derive(Debug, Subcommand)]
pub enum AlertCommand {
    /// List alert rules.
    List,
}

#[derive(Debug, Subcommand)]
pub enum TeamCommand {
    /// List the instance's users.
    List,
}

#[derive(Debug, Subcommand)]
pub enum TokenCommand {
    /// List your tokens (masked).
    List,
    /// Mint a token, from the stored one.
    Create(TokenCreateArgs),
    /// Revoke a token.
    Revoke(TokenRevokeArgs),
}

#[derive(Debug, Args)]
pub struct TokenCreateArgs {
    #[arg(long, value_name = "TEXT")]
    pub description: Option<String>,

    /// Weeks until it expires. Never, when omitted.
    #[arg(long, value_name = "WEEKS")]
    pub weeks: Option<i64>,
}

#[derive(Debug, Args)]
pub struct TokenRevokeArgs {
    pub id: i32,

    #[arg(long, short = 'y')]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct SendArgs {
    /// The project DSN, as the dashboard shows it.
    #[arg(long, value_name = "DSN", conflicts_with = "project")]
    pub dsn: Option<String>,

    /// The error message to send.
    #[arg(default_value = "Test event from rustrak CLI", value_name = "MESSAGE")]
    pub message: String,

    /// The level: `fatal`, `error`, `warning`, `info`, `debug`.
    #[arg(long, default_value = "error", value_name = "LEVEL")]
    pub level: String,

    /// The environment tag.
    #[arg(long, value_name = "ENV")]
    pub environment: Option<String>,

    /// The release tag.
    #[arg(long, value_name = "RELEASE")]
    pub release: Option<String>,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

#[derive(Debug, Args)]
pub struct ApiArgs {
    /// The path, starting with `/`. `/api` is assumed when omitted.
    pub path: String,

    /// The HTTP method.
    #[arg(long, value_enum, default_value_t = Method::Get)]
    pub method: Method,

    /// A JSON body, for POST, PUT, PATCH and DELETE.
    #[arg(long, value_name = "JSON")]
    pub body: Option<String>,
}

#[derive(Debug, Args)]
pub struct CompletionArgs {
    /// The shell to generate for.
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,
}
