import type { KyInstance } from 'ky';
import type { ClientConfig } from './config.js';
import {
  AgentsResource,
  AlertIntegrationsResource,
  AlertRulesResource,
  AuthResource,
  EventsResource,
  GitHubResource,
  HealthResource,
  InvitationsResource,
  IssuesResource,
  LogsResource,
  MembersResource,
  MonitorsResource,
  ProjectsResource,
  ReleasesResource,
  SchedulesResource,
  SessionsResource,
  SourceMapsResource,
  SpansResource,
  StatsResource,
  StorageResource,
  TeamResource,
  TokensResource,
  TransactionsResource,
} from './resources/index.js';
import { createKyInstance } from './utils/index.js';

/**
 * Main Rustrak API client
 *
 * @example
 * ```typescript
 * const client = new RustrakClient({
 *   baseUrl: 'http://localhost:8080',
 *   token: 'your-api-token'
 * });
 *
 * // List all projects
 * const projects = await client.projects.list();
 *
 * // Get issues for a project
 * const issues = await client.issues.list(1);
 *
 * // Get events for an issue
 * const events = await client.events.list(1, 'issue-uuid');
 * ```
 */
export class RustrakClient {
  private readonly http: KyInstance;

  /**
   * Authentication API resource
   */
  public readonly auth: AuthResource;

  /**
   * Projects API resource
   */
  public readonly projects: ProjectsResource;

  /**
   * Issues API resource
   */
  public readonly issues: IssuesResource;

  /**
   * Events API resource
   */
  public readonly events: EventsResource;

  /**
   * Auth Tokens API resource
   */
  public readonly tokens: TokensResource;

  /**
   * Alert Integrations API resource (global credential destinations)
   */
  public readonly alertIntegrations: AlertIntegrationsResource;

  /**
   * Alert Rules API resource (per-project alert configuration)
   */
  public readonly alertRules: AlertRulesResource;

  /**
   * Source Maps API resource (sentry-cli artifact bundle upload protocol)
   */
  public readonly sourceMaps: SourceMapsResource;

  /**
   * Team API resource (global user roster and roles)
   */
  public readonly team: TeamResource;

  /**
   * Invitations API resource (pending user invitations)
   */
  public readonly invitations: InvitationsResource;

  /**
   * Project Members API resource (per-project membership and roles)
   */
  public readonly members: MembersResource;

  /**
   * Sessions API resource (release health)
   */
  public readonly sessions: SessionsResource;

  /**
   * Releases API resource (data scoped to a specific release)
   */
  public readonly releases: ReleasesResource;

  /**
   * Transactions API resource (performance monitoring)
   */
  public readonly transactions: TransactionsResource;

  /**
   * Logs API resource (standalone logs)
   */
  public readonly logs: LogsResource;

  /**
   * Cron monitors API resource (Sentry Crons)
   */
  public readonly monitors: MonitorsResource;

  /**
   * Schedules API resource (previews for crontab/interval schedules)
   */
  public readonly schedules: SchedulesResource;

  /**
   * Spans API resource (standalone + transaction-embedded spans, shared table)
   */
  public readonly spans: SpansResource;

  /**
   * AI Agent Monitoring dashboard API resource
   */
  public readonly agents: AgentsResource;

  /**
   * Stats API resource (project-wide overview aggregates)
   */
  public readonly stats: StatsResource;

  /**
   * Storage API resource (usage + retention cleanup, admin only)
   */
  public readonly storage: StorageResource;

  /**
   * Health API resource (version info)
   */
  public readonly health: HealthResource;

  /**
   * GitHub integration resource
   */
  public readonly github: GitHubResource;

  /**
   * Create a new Rustrak API client
   *
   * @param config - Client configuration
   */
  constructor(config: ClientConfig) {
    this.http = createKyInstance(config);

    // Initialize resources
    this.auth = new AuthResource(this.http);
    this.projects = new ProjectsResource(this.http);
    this.issues = new IssuesResource(this.http);
    this.events = new EventsResource(this.http);
    this.tokens = new TokensResource(this.http);
    this.github = new GitHubResource(this.http);
    this.alertIntegrations = new AlertIntegrationsResource(this.http);
    this.alertRules = new AlertRulesResource(this.http);
    this.sourceMaps = new SourceMapsResource(this.http);
    this.team = new TeamResource(this.http);
    this.invitations = new InvitationsResource(this.http);
    this.members = new MembersResource(this.http);
    this.sessions = new SessionsResource(this.http);
    this.releases = new ReleasesResource(this.http);
    this.transactions = new TransactionsResource(this.http);
    this.logs = new LogsResource(this.http);
    this.monitors = new MonitorsResource(this.http);
    this.schedules = new SchedulesResource(this.http);
    this.spans = new SpansResource(this.http);
    this.agents = new AgentsResource(this.http);
    this.stats = new StatsResource(this.http);
    this.storage = new StorageResource(this.http);
    this.health = new HealthResource(this.http);
  }
}
