import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast } from '../../e2e/helpers';
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

test.describe.serial('EffectiveDate Employee View', () => {
  let createdId: string;
  const data = testData();

  test.beforeAll(async ({ orgContext }) => {


    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });




  test('employee can see EffectiveDate list', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    const table = employeePage.locator('[data-testid="effective_date-table"]');
    const empty = employeePage.locator('[data-testid="effective_date-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('employee can view EffectiveDate detail', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="effective_date-field-valid_from"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="effective_date-field-valid_to"]')).toBeVisible();
  });



  test('employee cannot access create form', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    await expect(employeePage.locator('[data-testid="effective_date-create-btn"]')).toBeHidden();
  });



  test('employee cannot access edit form', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="effective_date-edit-btn"]')).toBeHidden();
  });



  test('employee cannot delete EffectiveDate', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="effective_date-delete-btn"]')).toBeHidden();
  });

});
