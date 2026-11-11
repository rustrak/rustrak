export interface ClientConfig {
  /** `http://localhost:8080` or `https://rustrak.example.com`. */
  baseUrl: string;

  /** An API token. Omit it to authenticate with the session cookie. */
  token?: string;

  /**
   * Per attempt, in milliseconds.
   * @default 30000
   */
  timeout?: number;

  /**
   * Repeats of a GET, HEAD, PUT or DELETE that failed transiently.
   * @default 2
   */
  maxRetries?: number;

  headers?: Record<string, string>;
}
