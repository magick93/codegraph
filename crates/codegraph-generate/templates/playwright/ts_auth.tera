// Auth setup for E2E tests
// Provides an auth token via API key or magic link

import { test as setup } from '@playwright/test';

setup('authenticate', async ({ request }) => {
  const apiKey = process.env.TEST_API_KEY;
  if (apiKey) {
    process.env.TEST_AUTH_TOKEN = apiKey;
    console.log('[auth] E2E tests running with API key authentication');
    return;
  }

  console.log('[auth] No TEST_API_KEY set — E2E tests will run unauthenticated');
  console.log('[auth] Set TEST_API_KEY=sk_your_key to enable authenticated tests');
});
