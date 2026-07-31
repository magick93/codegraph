import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/common/identifier';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'scheme_agency_id': 'Test Scheme Agency Id',
    'scheme_id': 'Test Scheme Id',
    'scheme_version_id': 'Test Scheme Version Id',
    'value': 'Test Value',
  };
}

test.describe('Identifier Employee View', () => {
  let createdId: string;
  const data = testData();

  test.beforeAll(async ({ orgContext }) => {


    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });




  test('employee can see Identifier list', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    const table = employeePage.locator('[data-testid="identifier-table"]');
    const empty = employeePage.locator('[data-testid="identifier-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('employee can view Identifier detail', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="identifier-field-scheme_agency_id"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="identifier-field-scheme_id"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="identifier-field-scheme_version_id"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="identifier-field-value"]')).toBeVisible();
  });



  test('employee cannot access create form', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    await expect(employeePage.locator('[data-testid="identifier-create-btn"]')).toBeHidden();
  });



  test('employee cannot access edit form', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="identifier-edit-btn"]')).toBeHidden();
  });



  test('employee cannot delete Identifier', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="identifier-delete-btn"]')).toBeHidden();
  });

});
