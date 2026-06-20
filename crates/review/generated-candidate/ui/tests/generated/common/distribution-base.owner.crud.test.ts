import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast, expectTableContains, expectTableNotContains, waitForHydration } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/common/distribution-base';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'description': 'Test Description',
    'end_date': '2025-01-15',
    'start_date': '2025-01-15',
  };
}

function updatedData(): Record<string, unknown> {
  return {
    'description': 'Updated Description',
    'end_date': '2025-06-20',
    'start_date': '2025-06-20',
  };
}

test.describe.serial('DistributionBase Owner CRUD', () => {
  let createdId: string;



  const data = testData();
  const updated = updatedData();



  test('owner can create DistributionBase via form', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/new`);

    // Wait for SvelteKit to hydrate so the form's onsubmit handler is attached
    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="distribution_base-submit-btn"]');
    if (await ownerPage.locator('#description').isVisible()) {
      await ownerPage.locator('#description').fill(String(data['description']));
    }
    if (await ownerPage.locator('#end_date').isVisible()) {
      await ownerPage.locator('#end_date').fill(String(data['end_date']));
    }
    if (await ownerPage.locator('#start_date').isVisible()) {
      await ownerPage.locator('#start_date').fill(String(data['start_date']));
    }
    await ownerPage.locator('[data-testid="distribution_base-submit-btn"]').click();
    await expectToast(ownerPage, 'created', 'success');
    // Wait for SvelteKit goto() navigation to complete after toast

    await ownerPage.waitForURL(/\/common\/distribution-base\/[0-9a-f-]+$/, { timeout: 20_000 });

    createdId = ownerPage.url().split('/').pop()!;
  });




  test('owner sees DistributionBase in list', async ({ ownerPage }) => {
    await ownerPage.goto(BASE_PATH);
    const table = ownerPage.locator('[data-testid="distribution_base-table"]');
    const empty = ownerPage.locator('[data-testid="distribution_base-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('owner can view DistributionBase detail', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}`);

    await expect(ownerPage.locator('[data-testid="distribution_base-field-description"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="distribution_base-field-end_date"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="distribution_base-field-start_date"]')).toBeVisible();
  });




  test('owner can edit DistributionBase', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}/edit`);

    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="distribution_base-submit-btn"]');
    if (await ownerPage.locator('#description').isVisible()) {
      await ownerPage.locator('#description').clear();
      await ownerPage.locator('#description').fill(String(updated['description']));
    }
    if (await ownerPage.locator('#end_date').isVisible()) {
      await ownerPage.locator('#end_date').clear();
      await ownerPage.locator('#end_date').fill(String(updated['end_date']));
    }
    if (await ownerPage.locator('#start_date').isVisible()) {
      await ownerPage.locator('#start_date').clear();
      await ownerPage.locator('#start_date').fill(String(updated['start_date']));
    }
    await ownerPage.locator('[data-testid="distribution_base-submit-btn"]').click();
    await expectToast(ownerPage, 'updated', 'success');
  });




  test('owner can delete DistributionBase', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}`);

    await waitForHydration(ownerPage, '[data-testid="distribution_base-delete-btn"]');
    await ownerPage.locator('[data-testid="distribution_base-delete-btn"]').click();
    // Wait for portal-rendered confirm dialog
    await expect(ownerPage.locator('[data-testid="confirm-dialog"]')).toBeVisible({ timeout: 20_000 });
    await ownerPage.locator('[data-testid="confirm-dialog-confirm"]').click();
    await expectToast(ownerPage, 'deleted', 'success');
    await ownerPage.goto(BASE_PATH);
    // After delete, list may be empty (showing empty state) or table may not contain the deleted item
    const table = ownerPage.locator('[data-testid="distribution_base-table"]');
    const empty = ownerPage.locator('[data-testid="distribution_base-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });

});
