import * as Sentry from '@sentry/node';
import { DSN } from '../../src/env.js';

Sentry.init({
  dsn: DSN,
  tracesSampleRate: 0,
  defaultIntegrations: false,
  integrations: [Sentry.contextLinesIntegration()],
});

// ---------------------------------------------------------------------------
// Fake data layer
// ---------------------------------------------------------------------------

const USERS: Record<string, { id: string; name: string; role: string }> = {
  'user-1': { id: 'user-1', name: 'Alice', role: 'admin' },
  'user-2': { id: 'user-2', name: 'Bob', role: 'viewer' },
};

const ORDERS: Record<string, { id: string; userId: string; total: number }> = {
  'order-42': { id: 'order-42', userId: 'user-1', total: 199 },
};

// ---------------------------------------------------------------------------
// Business logic
// ---------------------------------------------------------------------------

function getUser(userId: string) {
  const user = USERS[userId];
  if (!user) throw new Error(`User not found: ${userId}`);
  return user;
}

function getOrder(orderId: string) {
  const order = ORDERS[orderId];
  if (!order) throw new Error(`Order not found: ${orderId}`);
  return order;
}

function generateInvoice(orderId: string) {
  const order = getOrder(orderId);
  const user = getUser(order.userId);
  return {
    invoiceNumber: `INV-${orderId}`,
    customer: user.name,
    total: order.total,
  };
}

// ---------------------------------------------------------------------------
// Entry point — triggers a real error deep in the call stack
// ---------------------------------------------------------------------------

async function main() {
  console.log('Running source map demo...');

  try {
    // This succeeds
    const inv = generateInvoice('order-42');
    console.log(`Invoice OK: ${inv.invoiceNumber} for ${inv.customer}`);

    // This throws: order-999 does not exist → getOrder throws
    generateInvoice('order-999');
  } catch (err) {
    const eventId = Sentry.captureException(err);
    console.log(`\nEvent captured: ${eventId}`);
    console.log('Check the Rustrak dashboard — the stack trace should show');
    console.log('  demo/src/app.ts  generateInvoice / getOrder');
    console.log('instead of the minified bundle line.');
  }

  await Sentry.flush(5000);
}

main();
