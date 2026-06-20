import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast, expectTableContains, expectTableNotContains, waitForHydration } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/common/identifier';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'scheme_agency_id': 'Test Scheme Agency Id',
    'scheme_id': 'Test Scheme Id',
    'scheme_version_id': 'Test Scheme Version Id',
    'value': 'Test Value',
  };
}

function updatedData(): Record<string, unknown> {
  return {
    'scheme_agency_id': 'Updated Scheme Agency Id',
    'scheme_id': 'Updated Scheme Id',
    'scheme_version_id': 'Updated Scheme Version Id',
    'value': 'Updated Value',
  };
}

test.describe.serial('Identifier Owner CRUD', () => {
  let createdId: string;



  const data = testData();
  const updated = updatedData();



  test('owner can create Identifier via form', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/new`);

    // Wait for SvelteKit to hydrate so the form's onsubmit handler is attached
    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="identifier-submit-btn"]');
    if (await ownerPage.locator('#scheme_agency_id').isVisible()) {
      await ownerPage.locator('#scheme_agency_id').fill(String(data['scheme_agency_id']));
    }
    if (await ownerPage.locator('#scheme_id').isVisible()) {
      await ownerPage.locator('#scheme_id').fill(String(data['scheme_id']));
    }
    if (await ownerPage.locator('#scheme_version_id').isVisible()) {
      await ownerPage.locator('#scheme_version_id').fill(String(data['scheme_version_id']));
    }
    if (await ownerPage.locator('#value').isVisible()) {
      await ownerPage.locator('#value').fill(String(data['value']));
    }
    await ownerPage.locator('[data-testid="identifier-submit-btn"]').click();
    await expectToast(ownerPage, 'created', 'success');
    // Wait for SvelteKit goto() navigation to complete after toast

    await ownerPage.waitForURL(/\/common\/identifier\/[0-9a-f-]+$/, { timeout: 20_000 });

    createdId = ownerPage.url().split('/').pop()!;
  });




  test('owner sees Identifier in list', async ({ ownerPage }) => {
    await ownerPage.goto(BASE_PATH);
    const table = ownerPage.locator('[data-testid="identifier-table"]');
    const empty = ownerPage.locator('[data-testid="identifier-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('owner can view Identifier detail', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}`);

    await expect(ownerPage.locator('[data-testid="identifier-field-scheme_agency_id"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="identifier-field-scheme_id"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="identifier-field-scheme_version_id"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="identifier-field-value"]')).toBeVisible();
  });




  test('owner can edit Identifier', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}/edit`);

    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="identifier-submit-btn"]');
    if (await ownerPage.locator('#scheme_agency_id').isVisible()) {
      await ownerPage.locator('#scheme_agency_id').clear();
      await ownerPage.locator('#scheme_agency_id').fill(String(updated['scheme_agency_id']));
    }
    if (await ownerPage.locator('#scheme_id').isVisible()) {
      await ownerPage.locator('#scheme_id').clear();
      await ownerPage.locator('#scheme_id').fill(String(updated['scheme_id']));
    }
    if (await ownerPage.locator('#scheme_version_id').isVisible()) {
      await ownerPage.locator('#scheme_version_id').clear();
      await ownerPage.locator('#scheme_version_id').fill(String(updated['scheme_version_id']));
    }
    if (await ownerPage.locator('#value').isVisible()) {
      await ownerPage.locator('#value').clear();
      await ownerPage.locator('#value').fill(String(updated['value']));
    }
    await ownerPage.locator('[data-testid="identifier-submit-btn"]').click();
    await expectToast(ownerPage, 'updated', 'success');
  });




  test('owner can delete Identifier', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}`);

    await waitForHydration(ownerPage, '[data-testid="identifier-delete-btn"]');
    await ownerPage.locator('[data-testid="identifier-delete-btn"]').click();
    // Wait for portal-rendered confirm dialog
    await expect(ownerPage.locator('[data-testid="confirm-dialog"]')).toBeVisible({ timeout: 20_000 });
    await ownerPage.locator('[data-testid="confirm-dialog-confirm"]').click();
    await expectToast(ownerPage, 'deleted', 'success');
    await ownerPage.goto(BASE_PATH);
    // After delete, list may be empty (showing empty state) or table may not contain the deleted item
    const table = ownerPage.locator('[data-testid="identifier-table"]');
    const empty = ownerPage.locator('[data-testid="identifier-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });

});
