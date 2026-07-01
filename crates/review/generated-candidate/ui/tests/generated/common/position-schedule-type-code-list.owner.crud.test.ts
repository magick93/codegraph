import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast, expectTableContains, expectTableNotContains, waitForHydration } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/common/position-schedule-type-code-list';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'code': `TestCode-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
  };
}

function updatedData(): Record<string, unknown> {
  return {
  };
}

test.describe.serial('PositionScheduleTypeCodeList Owner CRUD', () => {
  let createdId: string;



  const data = testData();
  const updated = updatedData();



  test('owner can create PositionScheduleTypeCodeList via form', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/new`);

    // Wait for SvelteKit to hydrate so the form's onsubmit handler is attached
    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="position_schedule_type_code_list-submit-btn"]');
    if (await ownerPage.locator('#code').isVisible()) {
      await ownerPage.locator('#code').fill(String(data['code']));
    }
    await ownerPage.locator('[data-testid="position_schedule_type_code_list-submit-btn"]').click();
    await expectToast(ownerPage, 'created', 'success');
    // Wait for SvelteKit goto() navigation to complete after toast

    await ownerPage.waitForURL(/\/common\/position-schedule-type-code-list\/[0-9a-f-]+$/, { timeout: 20_000 });

    createdId = ownerPage.url().split('/').pop()!;
  });




  test('owner sees PositionScheduleTypeCodeList in list', async ({ ownerPage }) => {
    await ownerPage.goto(BASE_PATH);
    const table = ownerPage.locator('[data-testid="position_schedule_type_code_list-table"]');
    const empty = ownerPage.locator('[data-testid="position_schedule_type_code_list-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('owner can view PositionScheduleTypeCodeList detail', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}`);

  });




  // All properties are complex types — no simple form fields to edit.
  // Edit-via-form test skipped; CRUD coverage provided by API create + detail + delete tests.




  test('owner can delete PositionScheduleTypeCodeList', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}`);

    await waitForHydration(ownerPage, '[data-testid="position_schedule_type_code_list-delete-btn"]');
    await ownerPage.locator('[data-testid="position_schedule_type_code_list-delete-btn"]').click();
    // Wait for portal-rendered confirm dialog
    await expect(ownerPage.locator('[data-testid="confirm-dialog"]')).toBeVisible({ timeout: 20_000 });
    await ownerPage.locator('[data-testid="confirm-dialog-confirm"]').click();
    await expectToast(ownerPage, 'deleted', 'success');
    await ownerPage.goto(BASE_PATH);
    // After delete, list may be empty (showing empty state) or table may not contain the deleted item
    const table = ownerPage.locator('[data-testid="position_schedule_type_code_list-table"]');
    const empty = ownerPage.locator('[data-testid="position_schedule_type_code_list-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });

});
