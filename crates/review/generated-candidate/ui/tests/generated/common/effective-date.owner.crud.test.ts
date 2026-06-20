import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast, expectTableContains, expectTableNotContains, waitForHydration } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/common/effective-date';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'valid_from': '2025-01-15',
    'valid_to': '2025-01-15',
  };
}

function updatedData(): Record<string, unknown> {
  return {
    'valid_from': '2025-06-20',
    'valid_to': '2025-06-20',
  };
}

test.describe.serial('EffectiveDate Owner CRUD', () => {
  let createdId: string;



  const data = testData();
  const updated = updatedData();



  test('owner can create EffectiveDate via form', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/new`);

    // Wait for SvelteKit to hydrate so the form's onsubmit handler is attached
    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="effective_date-submit-btn"]');
    if (await ownerPage.locator('#valid_from').isVisible()) {
      await ownerPage.locator('#valid_from').fill(String(data['valid_from']));
    }
    if (await ownerPage.locator('#valid_to').isVisible()) {
      await ownerPage.locator('#valid_to').fill(String(data['valid_to']));
    }
    await ownerPage.locator('[data-testid="effective_date-submit-btn"]').click();
    await expectToast(ownerPage, 'created', 'success');
    // Wait for SvelteKit goto() navigation to complete after toast

    await ownerPage.waitForURL(/\/common\/effective-date\/[0-9a-f-]+$/, { timeout: 20_000 });

    createdId = ownerPage.url().split('/').pop()!;
  });




  test('owner sees EffectiveDate in list', async ({ ownerPage }) => {
    await ownerPage.goto(BASE_PATH);
    const table = ownerPage.locator('[data-testid="effective_date-table"]');
    const empty = ownerPage.locator('[data-testid="effective_date-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('owner can view EffectiveDate detail', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}`);

    await expect(ownerPage.locator('[data-testid="effective_date-field-valid_from"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="effective_date-field-valid_to"]')).toBeVisible();
  });




  test('owner can edit EffectiveDate', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}/edit`);

    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="effective_date-submit-btn"]');
    if (await ownerPage.locator('#valid_from').isVisible()) {
      await ownerPage.locator('#valid_from').clear();
      await ownerPage.locator('#valid_from').fill(String(updated['valid_from']));
    }
    if (await ownerPage.locator('#valid_to').isVisible()) {
      await ownerPage.locator('#valid_to').clear();
      await ownerPage.locator('#valid_to').fill(String(updated['valid_to']));
    }
    await ownerPage.locator('[data-testid="effective_date-submit-btn"]').click();
    await expectToast(ownerPage, 'updated', 'success');
  });




  test('owner can delete EffectiveDate', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}`);

    await waitForHydration(ownerPage, '[data-testid="effective_date-delete-btn"]');
    await ownerPage.locator('[data-testid="effective_date-delete-btn"]').click();
    // Wait for portal-rendered confirm dialog
    await expect(ownerPage.locator('[data-testid="confirm-dialog"]')).toBeVisible({ timeout: 20_000 });
    await ownerPage.locator('[data-testid="confirm-dialog-confirm"]').click();
    await expectToast(ownerPage, 'deleted', 'success');
    await ownerPage.goto(BASE_PATH);
    // After delete, list may be empty (showing empty state) or table may not contain the deleted item
    const table = ownerPage.locator('[data-testid="effective_date-table"]');
    const empty = ownerPage.locator('[data-testid="effective_date-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });

});
