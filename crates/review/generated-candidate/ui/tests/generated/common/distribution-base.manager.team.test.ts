import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi } from '../../e2e/helpers';
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

test.describe.serial('DistributionBase Manager Team', () => {
  let createdId: string;
  const data = testData();

  test.beforeAll(async ({ orgContext }) => {


    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });




  test('manager can see DistributionBase list', async ({ managerPage }) => {
    await managerPage.goto(BASE_PATH);
    const table = managerPage.locator('[data-testid="distribution_base-table"]');
    const empty = managerPage.locator('[data-testid="distribution_base-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('manager can view DistributionBase detail', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}`);

    await expect(managerPage.locator('[data-testid="distribution_base-field-description"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="distribution_base-field-end_date"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="distribution_base-field-start_date"]')).toBeVisible();
  });



  test('manager can edit DistributionBase', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}/edit`);

    // Manager should see the edit form — verifies team-scoped write access
    await expect(managerPage.locator('[data-testid="distribution_base-form"]')).toBeVisible();
  });

});
