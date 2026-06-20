import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi } from '../../e2e/helpers';
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

test.describe.serial('EffectiveDate Manager Team', () => {
  let createdId: string;
  const data = testData();

  test.beforeAll(async ({ orgContext }) => {


    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });




  test('manager can see EffectiveDate list', async ({ managerPage }) => {
    await managerPage.goto(BASE_PATH);
    const table = managerPage.locator('[data-testid="effective_date-table"]');
    const empty = managerPage.locator('[data-testid="effective_date-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('manager can view EffectiveDate detail', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}`);

    await expect(managerPage.locator('[data-testid="effective_date-field-valid_from"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="effective_date-field-valid_to"]')).toBeVisible();
  });



  test('manager can edit EffectiveDate', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}/edit`);

    // Manager should see the edit form — verifies team-scoped write access
    await expect(managerPage.locator('[data-testid="effective_date-form"]')).toBeVisible();
  });

});
