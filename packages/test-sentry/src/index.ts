import * as Sentry from '@sentry/node';

export function initSentry(config: {
  dsn: string;
  debug: boolean;
  environment: string;
  release?: string;
}): void {
  Sentry.init({
    dsn: config.dsn,
    debug: config.debug,
    environment: config.environment,
    release: config.release ?? 'test-sentry@1.0.0',
    tracesSampleRate: 1.0,
    defaultIntegrations: false,
    integrations: [
      Sentry.consoleIntegration(),
      Sentry.httpIntegration(),
      Sentry.contextLinesIntegration(),
    ],
  });
}

export function testCapturedError(): void {
  const eventId = Sentry.captureException(
    new Error('Test captured error from test-sentry package'),
  );
  console.log(`[testCapturedError] Sent event: ${eventId}`);
}

export function testTypeError(): void {
  try {
    const obj: unknown = null;
    // @ts-expect-error intentional error for testing
    obj.someProperty.nested;
  } catch (error) {
    const eventId = Sentry.captureException(error);
    console.log(`[testTypeError] Sent event: ${eventId}`);
  }
}

export function testReferenceError(): void {
  try {
    // @ts-expect-error intentional error for testing
    console.log(undefinedVariable);
  } catch (error) {
    const eventId = Sentry.captureException(error);
    console.log(`[testReferenceError] Sent event: ${eventId}`);
  }
}

class CustomApplicationError extends Error {
  constructor(
    message: string,
    public readonly code: string,
    public readonly details?: Record<string, unknown>,
  ) {
    super(message);
    this.name = 'CustomApplicationError';
  }
}

export function testCustomError(): void {
  const error = new CustomApplicationError(
    'Something went wrong in the application',
    'ERR_CUSTOM_001',
    { userId: 123, action: 'test' },
  );
  const eventId = Sentry.captureException(error);
  console.log(`[testCustomError] Sent event: ${eventId}`);
}

export function testCaptureMessage(): void {
  const eventId = Sentry.captureMessage(
    'This is a test message from test-sentry',
    'info',
  );
  console.log(`[testCaptureMessage] Sent event: ${eventId}`);
}

export function testWithBreadcrumbs(): void {
  Sentry.addBreadcrumb({
    category: 'auth',
    message: 'User logged in',
    level: 'info',
    data: { userId: 'user-123' },
  });

  Sentry.addBreadcrumb({
    category: 'navigation',
    message: 'User navigated to /dashboard',
    level: 'info',
  });

  Sentry.addBreadcrumb({
    category: 'ui.click',
    message: 'User clicked on "Submit" button',
    level: 'info',
  });

  Sentry.addBreadcrumb({
    category: 'http',
    message: 'GET /api/data',
    level: 'info',
    data: {
      url: '/api/data',
      method: 'GET',
      status_code: 200,
    },
  });

  Sentry.addBreadcrumb({
    category: 'http',
    message: 'POST /api/submit',
    level: 'error',
    data: {
      url: '/api/submit',
      method: 'POST',
      status_code: 500,
    },
  });

  const eventId = Sentry.captureException(
    new Error('Error after breadcrumb trail'),
  );
  console.log(`[testWithBreadcrumbs] Sent event: ${eventId}`);
}

export function testWithUserContext(): void {
  Sentry.setUser({
    id: 'user-456',
    email: 'testuser@example.com',
    username: 'testuser',
    ip_address: '192.168.1.100',
  });

  const eventId = Sentry.captureException(
    new Error('Error with user context attached'),
  );
  console.log(`[testWithUserContext] Sent event: ${eventId}`);

  Sentry.setUser(null);
}

export function testWithTags(): void {
  Sentry.setTag('environment', 'testing');
  Sentry.setTag('feature', 'error-tracking');
  Sentry.setTag('version', '2.0.0');

  Sentry.withScope((scope) => {
    scope.setTag('scope_specific_tag', 'scoped_value');
    scope.setLevel('warning');

    const eventId = Sentry.captureException(
      new Error('Error with custom tags'),
    );
    console.log(`[testWithTags] Sent event: ${eventId}`);
  });
}

export function testWithExtraContext(): void {
  Sentry.setExtra('request_id', 'req-789');
  Sentry.setExtra('timestamp', new Date().toISOString());

  Sentry.withScope((scope) => {
    scope.setExtras({
      user_action: 'form_submit',
      form_data: {
        field1: 'value1',
        field2: 'value2',
      },
      processing_time_ms: 1234,
    });

    const eventId = Sentry.captureException(
      new Error('Error with extra context'),
    );
    console.log(`[testWithExtraContext] Sent event: ${eventId}`);
  });
}

export function testWithFingerprint(): void {
  Sentry.withScope((scope) => {
    scope.setFingerprint(['custom-group', 'database-error']);

    const eventId = Sentry.captureException(
      new Error('Database connection failed - custom fingerprint'),
    );
    console.log(`[testWithFingerprint] Sent event: ${eventId}`);
  });
}

export function testWithTransaction(): void {
  Sentry.withScope((scope) => {
    scope.setTransactionName('POST /api/users/create');

    const eventId = Sentry.captureException(
      new Error('Error during user creation'),
    );
    console.log(`[testWithTransaction] Sent event: ${eventId}`);
  });
}

export function testNestedError(): void {
  try {
    try {
      throw new Error('Root cause: database timeout');
    } catch (dbError) {
      const serviceError = new Error('Service layer error');
      serviceError.cause = dbError;
      throw serviceError;
    }
  } catch (error) {
    const eventId = Sentry.captureException(error);
    console.log(`[testNestedError] Sent event: ${eventId}`);
  }
}

export async function testAsyncError(): Promise<void> {
  async function innerAsync(): Promise<void> {
    await new Promise((resolve) => setTimeout(resolve, 10));
    throw new Error('Async operation failed');
  }

  async function middleAsync(): Promise<void> {
    await innerAsync();
  }

  try {
    await middleAsync();
  } catch (error) {
    const eventId = Sentry.captureException(error);
    console.log(`[testAsyncError] Sent event: ${eventId}`);
  }
}

export function testMultipleErrors(): void {
  for (let i = 1; i <= 5; i++) {
    const eventId = Sentry.captureException(
      new Error(`Sequential error #${i}`),
    );
    console.log(`[testMultipleErrors] Sent event #${i}: ${eventId}`);
  }
}

export function testErrorFlood(count: number): void {
  console.log(`[testErrorFlood] Sending ${count} errors...`);

  for (let i = 1; i <= count; i++) {
    Sentry.captureException(
      new Error(`Flood error #${i} - testing rate limits`),
    );
  }

  console.log(`[testErrorFlood] Sent ${count} events`);
}

export function testErrorLevels(): void {
  const levels: Sentry.SeverityLevel[] = [
    'fatal',
    'error',
    'warning',
    'info',
    'debug',
  ];

  for (const level of levels) {
    Sentry.withScope((scope) => {
      scope.setLevel(level);
      const eventId = Sentry.captureMessage(
        `Test message with level: ${level}`,
        level,
      );
      console.log(`[testErrorLevels] Sent ${level} event: ${eventId}`);
    });
  }
}

export function testWithContexts(): void {
  Sentry.withScope((scope) => {
    scope.setContext('device', {
      name: 'Test Device',
      family: 'Desktop',
      model: 'MacBook Pro',
      brand: 'Apple',
    });

    scope.setContext('os', {
      name: 'macOS',
      version: '14.0',
    });

    scope.setContext('app', {
      app_name: 'Rustrak Test',
      app_version: '1.0.0',
      app_build: '100',
    });

    scope.setContext('custom', {
      feature_flags: {
        new_dashboard: true,
        beta_features: false,
      },
      experiment_id: 'exp-123',
    });

    const eventId = Sentry.captureException(
      new Error('Error with rich context'),
    );
    console.log(`[testWithContexts] Sent event: ${eventId}`);
  });
}
