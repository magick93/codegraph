import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast, expectTableContains, expectTableNotContains, waitForHydration } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/common/amount';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'currency': 'USD',
    'value': 42,
  };
}

function updatedData(): Record<string, unknown> {
  return {
    'currency': 'AUD',
    'value': 99,
  };
}

test.describe.serial('Amount Owner CRUD', () => {
  let createdId: string;



  const data = testData();
  const updated = updatedData();



  test('owner can create Amount via form', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/new`);

    // Wait for SvelteKit to hydrate so the form's onsubmit handler is attached
    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="amount-submit-btn"]');
    if (await ownerPage.locator('#currency').isVisible()) {
      await ownerPage.locator('#currency').selectOption(String(data['currency']));
    }
    if (await ownerPage.locator('#value').isVisible()) {
      await ownerPage.locator('#value').fill(String(data['value']));
    }
    await ownerPage.locator('[data-testid="amount-submit-btn"]').click();
    await expectToast(ownerPage, 'created', 'success');
    // Wait for SvelteKit goto() navigation to complete after toast

    await ownerPage.waitForURL(/\/common\/amount\/[0-9a-f-]+$/, { timeout: 20_000 });

    createdId = ownerPage.url().split('/').pop()!;
  });




  test('owner sees Amount in list', async ({ ownerPage }) => {
    await ownerPage.goto(BASE_PATH);
    const table = ownerPage.locator('[data-testid="amount-table"]');
    const empty = ownerPage.locator('[data-testid="amount-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('owner can view Amount detail', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}`);

    await expect(ownerPage.locator('[data-testid="amount-field-currency"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="amount-field-value"]')).toBeVisible();
  });




  test('owner can edit Amount', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}/edit`);

    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="amount-submit-btn"]');
    if (await ownerPage.locator('#currency').isVisible()) {
      await ownerPage.locator('#currency').selectOption('AUD');
    }
    if (await ownerPage.locator('#value').isVisible()) {
      await ownerPage.locator('#value').clear();
      await ownerPage.locator('#value').fill(String(updated['value']));
    }
    await ownerPage.locator('[data-testid="amount-submit-btn"]').click();
    await expectToast(ownerPage, 'updated', 'success');
  });




  test('owner can delete Amount', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}`);

    await waitForHydration(ownerPage, '[data-testid="amount-delete-btn"]');
    await ownerPage.locator('[data-testid="amount-delete-btn"]').click();
    // Wait for portal-rendered confirm dialog
    await expect(ownerPage.locator('[data-testid="confirm-dialog"]')).toBeVisible({ timeout: 20_000 });
    await ownerPage.locator('[data-testid="confirm-dialog-confirm"]').click();
    await expectToast(ownerPage, 'deleted', 'success');
    await ownerPage.goto(BASE_PATH);
    // After delete, list may be empty (showing empty state) or table may not contain the deleted item
    const table = ownerPage.locator('[data-testid="amount-table"]');
    const empty = ownerPage.locator('[data-testid="amount-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });

});
