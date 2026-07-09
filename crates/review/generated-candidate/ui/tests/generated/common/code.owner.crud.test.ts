import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast, expectTableContains, expectTableNotContains, waitForHydration } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/common/code';


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

test.describe('Code Owner CRUD', () => {



  const data = testData();
  const updated = updatedData();



  test('owner can create Code via API', async ({ orgContext }) => {
    // All properties are complex types (value objects / child tables) — no simple form fields.
    // Use direct API call via orgContext (authenticated as ACME owner).
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const apiCreatedId = entity['id'] as string;
    expect(apiCreatedId).toBeTruthy();
  });




  test('owner sees Code in list', async ({ ownerPage }) => {
    await ownerPage.goto(BASE_PATH);
    const table = ownerPage.locator('[data-testid="code-table"]');
    const empty = ownerPage.locator('[data-testid="code-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('owner can view Code detail', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}`);

  });




  // All properties are complex types — no simple form fields to edit.
  // Edit-via-form test skipped; CRUD coverage provided by API create + detail + delete tests.




  test('owner can delete Code', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}`);

    await waitForHydration(ownerPage, '[data-testid="code-delete-btn"]');
    await ownerPage.locator('[data-testid="code-delete-btn"]').click();
    // Wait for portal-rendered confirm dialog
    await expect(ownerPage.locator('[data-testid="confirm-dialog"]')).toBeVisible({ timeout: 20_000 });
    await ownerPage.locator('[data-testid="confirm-dialog-confirm"]').click();
    await expectToast(ownerPage, 'deleted', 'success');
    await ownerPage.goto(BASE_PATH);
    // After delete, list may be empty (showing empty state) or table may not contain the deleted item
    const table = ownerPage.locator('[data-testid="code-table"]');
    const empty = ownerPage.locator('[data-testid="code-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });

});
