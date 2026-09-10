import { test as base, type Page, type APIRequestContext } from '@playwright/test';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const AUTH_DIR = path.resolve(__dirname, '..', 'auth');
const CONTEXT_FILE = path.join(AUTH_DIR, 'org-context.json');

export type OrgContext = {
  timestamp: number;
  acme: {
    orgId: string;
    ownerId: string;
    employeeId: string;
    managerId: string;
    apiKey: string;
    ownerEmail: string;
    employeeEmail: string;
    managerEmail: string;
  };
  highfive: {
    orgId: string;
    ownerId: string;
    ownerEmail: string;
    apiKey: string;
  };
};

function loadOrgContext(): OrgContext {
  return JSON.parse(fs.readFileSync(CONTEXT_FILE, 'utf-8'));
}

type PersonaFixtures = {
  ownerPage: Page;
  employeePage: Page;
  managerPage: Page;
  highfiveOwnerPage: Page;
  apiContext: APIRequestContext;
  orgContext: OrgContext;
};

/**
 * Resolve the worker-specific storage state file for a persona.
 * Each Playwright worker gets its own independent Supabase session to avoid
 * refresh-token contention when running tests in parallel.
 */
function workerStorageState(prefix: string, parallelIndex: number): string {
  const file = path.join(AUTH_DIR, `${prefix}-${parallelIndex}.storageState.json`);
  if (!fs.existsSync(file)) {
    throw new Error(
      `Storage state file not found: ${file}. ` +
      `Ensure PW_WORKER_COUNT (currently ${process.env.PW_WORKER_COUNT ?? 'unset'}) >= the workers setting.`,
    );
  }
  return file;
}

export const test = base.extend<PersonaFixtures>({
  orgContext: async ({}, use) => {
    await use(loadOrgContext());
  },

  ownerPage: async ({ browser }, use, testInfo) => {
    const ctx = await browser.newContext({
      storageState: workerStorageState('acme-owner', testInfo.parallelIndex),
    });
    const page = await ctx.newPage();
    await use(page);
    await ctx.close();
  },

  employeePage: async ({ browser }, use, testInfo) => {
    const ctx = await browser.newContext({
      storageState: workerStorageState('acme-employee', testInfo.parallelIndex),
    });
    const page = await ctx.newPage();
    await use(page);
    await ctx.close();
  },

  managerPage: async ({ browser }, use, testInfo) => {
    const ctx = await browser.newContext({
      storageState: workerStorageState('acme-manager', testInfo.parallelIndex),
    });
    const page = await ctx.newPage();
    await use(page);
    await ctx.close();
  },

  highfiveOwnerPage: async ({ browser }, use, testInfo) => {
    const ctx = await browser.newContext({
      storageState: workerStorageState('highfive-owner', testInfo.parallelIndex),
    });
    const page = await ctx.newPage();
    await use(page);
    await ctx.close();
  },

  apiContext: async ({ playwright }, use) => {
    const context = loadOrgContext();
    const ctx = await playwright.request.newContext({
      baseURL: process.env.PUBLIC_API_URL ?? 'http://localhost:3000',
      extraHTTPHeaders: {
        Authorization: `Bearer ${context.acme.apiKey}`,
      },
    });
    await use(ctx);
    await ctx.dispose();
  },
});

export { expect } from '@playwright/test';
