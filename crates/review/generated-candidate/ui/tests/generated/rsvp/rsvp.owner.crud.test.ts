import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast, expectTableContains, expectTableNotContains, waitForHydration } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/rsvp/rsvp';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    // 'event': ValueObject — omit, serde default
    'status': 'Confirmed',
    'timestamp': '2025-01-15T10:30:00Z',
  };
}

function updatedData(): Record<string, unknown> {
  return {
    // 'event': ValueObject — omit, serde default
    'status': 'Cancelled',
    'timestamp': '2025-06-20T14:00:00Z',
  };
}

test.describe('Rsvp Owner CRUD', () => {



  let data: Record<string, unknown>;
  let updated: Record<string, unknown>;
  test.beforeEach(() => {
    data = testData();
    updated = updatedData();
  });



  test('owner can create Rsvp via form', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/new`);

    // Wait for SvelteKit to hydrate so the form's onsubmit handler is attached
    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="rsvp-submit-btn"]');
    // 'event' is a ValueObject — provide nested structure in testData()
    if (await ownerPage.locator('#status').isVisible()) {
      await ownerPage.locator('#status').selectOption(String(data['status']));
    }
    if (await ownerPage.locator('#timestamp').isVisible()) {
      await ownerPage.locator('#timestamp').fill(String(data['timestamp']).replace(/:\d{2}Z$/, ''));
    }
    await ownerPage.locator('[data-testid="rsvp-submit-btn"]').click();
    await expectToast(ownerPage, 'created', 'success');
    // Wait for SvelteKit goto() navigation to complete after toast

    await ownerPage.waitForURL(/\/rsvp\/rsvp\/[0-9a-f-]+$/, { timeout: 20_000 });

    const formCreatedId = ownerPage.url().split('/').pop()!;
  });




  test('owner sees Rsvp in list', async ({ ownerPage }) => {
    await ownerPage.goto(BASE_PATH);
    const table = ownerPage.locator('[data-testid="rsvp-table"]');
    const empty = ownerPage.locator('[data-testid="rsvp-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('owner can view Rsvp detail', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}`);

    await expect(ownerPage.locator('[data-testid="rsvp-field-event"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="rsvp-field-status"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="rsvp-field-timestamp"]')).toBeVisible();
  });




  test('owner can edit Rsvp', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}/edit`);

    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="rsvp-submit-btn"]');
    // 'event' is a ValueObject — provide nested structure in testData()
    if (await ownerPage.locator('#status').isVisible()) {
      await ownerPage.locator('#status').selectOption('Cancelled');
    }
    if (await ownerPage.locator('#timestamp').isVisible()) {
      await ownerPage.locator('#timestamp').clear();
      await ownerPage.locator('#timestamp').fill(String(updated['timestamp']).replace(/:\d{2}Z$/, ''));
    }
    await ownerPage.locator('[data-testid="rsvp-submit-btn"]').click();
    await expectToast(ownerPage, 'updated', 'success');
  });




  test('owner can delete Rsvp', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}`);

    await waitForHydration(ownerPage, '[data-testid="rsvp-delete-btn"]');
    await ownerPage.locator('[data-testid="rsvp-delete-btn"]').click();
    // Wait for portal-rendered confirm dialog
    await expect(ownerPage.locator('[data-testid="confirm-dialog"]')).toBeVisible({ timeout: 20_000 });
    await ownerPage.locator('[data-testid="confirm-dialog-confirm"]').click();
    await expectToast(ownerPage, 'deleted', 'success');
    await ownerPage.goto(BASE_PATH);
    // After delete, list may be empty (showing empty state) or table may not contain the deleted item
    const table = ownerPage.locator('[data-testid="rsvp-table"]');
    const empty = ownerPage.locator('[data-testid="rsvp-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });

});
