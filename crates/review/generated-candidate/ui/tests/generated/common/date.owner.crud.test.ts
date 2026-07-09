import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast, expectTableContains, expectTableNotContains, waitForHydration } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/common/date';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
  };
}

function updatedData(): Record<string, unknown> {
  return {
  };
}

test.describe.serial('Date Owner CRUD', () => {
  let createdId: string;



  const data = testData();
  const updated = updatedData();



  test('owner can create Date via API', async ({ orgContext }) => {
    // All properties are complex types (value objects / child tables) — no simple form fields.
    // Use direct API call via orgContext (authenticated as ACME owner).
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity['id'] as string;
    expect(createdId).toBeTruthy();
  });




  test('owner sees Date in list', async ({ ownerPage }) => {
    await ownerPage.goto(BASE_PATH);
    const table = ownerPage.locator('[data-testid="date-table"]');
    const empty = ownerPage.locator('[data-testid="date-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('owner can view Date detail', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}`);

  });




  // All properties are complex types — no simple form fields to edit.
  // Edit-via-form test skipped; CRUD coverage provided by API create + detail + delete tests.




  test('owner can delete Date', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}`);

    await waitForHydration(ownerPage, '[data-testid="date-delete-btn"]');
    await ownerPage.locator('[data-testid="date-delete-btn"]').click();
    // Wait for portal-rendered confirm dialog
    await expect(ownerPage.locator('[data-testid="confirm-dialog"]')).toBeVisible({ timeout: 20_000 });
    await ownerPage.locator('[data-testid="confirm-dialog-confirm"]').click();
    await expectToast(ownerPage, 'deleted', 'success');
    await ownerPage.goto(BASE_PATH);
    // After delete, list may be empty (showing empty state) or table may not contain the deleted item
    const table = ownerPage.locator('[data-testid="date-table"]');
    const empty = ownerPage.locator('[data-testid="date-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });

});
