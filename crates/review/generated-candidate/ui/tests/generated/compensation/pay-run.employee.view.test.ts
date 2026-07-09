import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast } from '../../e2e/helpers';
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

test.describe('PayRun Employee View', () => {
  let createdId: string;
  const data = testData();

  test.beforeAll(async ({ orgContext }) => {


    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });




  test('employee can see PayRun list', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    const table = employeePage.locator('[data-testid="pay_run-table"]');
    const empty = employeePage.locator('[data-testid="pay_run-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('employee can view PayRun detail', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="pay_run-field-pay_run_id"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="pay_run-field-run_date"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="pay_run-field-total_amount"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="pay_run-field-total_amount_currency"]')).toBeVisible();
  });



  test('employee cannot access create form', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    await expect(employeePage.locator('[data-testid="pay_run-create-btn"]')).toBeHidden();
  });



  test('employee cannot access edit form', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="pay_run-edit-btn"]')).toBeHidden();
  });



  test('employee cannot delete PayRun', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="pay_run-delete-btn"]')).toBeHidden();
  });

});
