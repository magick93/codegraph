import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/common/gender-code-list';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'code': `TestCode-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
    'display_name': 'Test Display Name',
    'sort_order': 42,
  };
}

test.describe.serial('GenderCodeList Manager Team', () => {
  let createdId: string;
  const data = testData();

  test.beforeAll(async ({ orgContext }) => {


    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });




  test('manager can see GenderCodeList list', async ({ managerPage }) => {
    await managerPage.goto(BASE_PATH);
    const table = managerPage.locator('[data-testid="gender_code_list-table"]');
    const empty = managerPage.locator('[data-testid="gender_code_list-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('manager can view GenderCodeList detail', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}`);

    await expect(managerPage.locator('[data-testid="gender_code_list-field-code"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="gender_code_list-field-display_name"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="gender_code_list-field-sort_order"]')).toBeVisible();
  });



  test('manager can edit GenderCodeList', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}/edit`);

    // Manager should see the edit form — verifies team-scoped write access
    await expect(managerPage.locator('[data-testid="gender_code_list-form"]')).toBeVisible();
  });

});
