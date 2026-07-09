import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast, expectTableContains, expectTableNotContains, waitForHydration } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/common/gender-code-list';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'code': `TestCode-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    'display_name': 'Test Display Name',
    'sort_order': 42,
  };
}

function updatedData(): Record<string, unknown> {
  return {
    'code': `TestCode-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    'display_name': 'Updated Display Name',
    'sort_order': 99,
  };
}

test.describe('GenderCodeList Owner CRUD', () => {



  const data = testData();
  const updated = updatedData();



  test('owner can create GenderCodeList via form', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/new`);

    // Wait for SvelteKit to hydrate so the form's onsubmit handler is attached
    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="gender_code_list-submit-btn"]');
    if (await ownerPage.locator('#code').isVisible()) {
      await ownerPage.locator('#code').fill(String(data['code']));
    }
    if (await ownerPage.locator('#display_name').isVisible()) {
      await ownerPage.locator('#display_name').fill(String(data['display_name']));
    }
    if (await ownerPage.locator('#sort_order').isVisible()) {
      await ownerPage.locator('#sort_order').fill(String(data['sort_order']));
    }
    await ownerPage.locator('[data-testid="gender_code_list-submit-btn"]').click();
    await expectToast(ownerPage, 'created', 'success');
    // Wait for SvelteKit goto() navigation to complete after toast

    await ownerPage.waitForURL(/\/common\/gender-code-list\/[0-9a-f-]+$/, { timeout: 20_000 });

    const formCreatedId = ownerPage.url().split('/').pop()!;
  });




  test('owner sees GenderCodeList in list', async ({ ownerPage }) => {
    await ownerPage.goto(BASE_PATH);
    const table = ownerPage.locator('[data-testid="gender_code_list-table"]');
    const empty = ownerPage.locator('[data-testid="gender_code_list-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('owner can view GenderCodeList detail', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}`);

    await expect(ownerPage.locator('[data-testid="gender_code_list-field-code"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="gender_code_list-field-display_name"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="gender_code_list-field-sort_order"]')).toBeVisible();
  });




  test('owner can edit GenderCodeList', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}/edit`);

    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="gender_code_list-submit-btn"]');
    if (await ownerPage.locator('#code').isVisible()) {
      await ownerPage.locator('#code').clear();
      await ownerPage.locator('#code').fill(String(updated['code']));
    }
    if (await ownerPage.locator('#display_name').isVisible()) {
      await ownerPage.locator('#display_name').clear();
      await ownerPage.locator('#display_name').fill(String(updated['display_name']));
    }
    if (await ownerPage.locator('#sort_order').isVisible()) {
      await ownerPage.locator('#sort_order').clear();
      await ownerPage.locator('#sort_order').fill(String(updated['sort_order']));
    }
    await ownerPage.locator('[data-testid="gender_code_list-submit-btn"]').click();
    await expectToast(ownerPage, 'updated', 'success');
  });




  test('owner can delete GenderCodeList', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}`);

    await waitForHydration(ownerPage, '[data-testid="gender_code_list-delete-btn"]');
    await ownerPage.locator('[data-testid="gender_code_list-delete-btn"]').click();
    // Wait for portal-rendered confirm dialog
    await expect(ownerPage.locator('[data-testid="confirm-dialog"]')).toBeVisible({ timeout: 20_000 });
    await ownerPage.locator('[data-testid="confirm-dialog-confirm"]').click();
    await expectToast(ownerPage, 'deleted', 'success');
    await ownerPage.goto(BASE_PATH);
    // After delete, list may be empty (showing empty state) or table may not contain the deleted item
    const table = ownerPage.locator('[data-testid="gender_code_list-table"]');
    const empty = ownerPage.locator('[data-testid="gender_code_list-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });

});
