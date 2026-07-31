import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/compensation/pay-run';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'pay_run_id': 'Test Pay Run Id',
    'run_date': '2025-01-15',
    'total_amount': 42,
    'total_amount_currency': 'USD',
  };
}

test.describe('PayRun Manager Team', () => {
  let createdId: string;
  const data = testData();

  test.beforeAll(async ({ orgContext }) => {


    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });




  test('manager can see PayRun list', async ({ managerPage }) => {
    await managerPage.goto(BASE_PATH);
    const table = managerPage.locator('[data-testid="pay_run-table"]');
    const empty = managerPage.locator('[data-testid="pay_run-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('manager can view PayRun detail', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}`);

    await expect(managerPage.locator('[data-testid="pay_run-field-pay_run_id"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="pay_run-field-run_date"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="pay_run-field-total_amount"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="pay_run-field-total_amount_currency"]')).toBeVisible();
  });



  test('manager can edit PayRun', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}/edit`);

    // Manager should see the edit form — verifies team-scoped write access
    await expect(managerPage.locator('[data-testid="pay_run-form"]')).toBeVisible();
  });

});
