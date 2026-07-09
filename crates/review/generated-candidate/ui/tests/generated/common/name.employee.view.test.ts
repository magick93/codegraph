import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/common/name';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'family_name': 'Test Family Name',
    'formatted_name': 'Test Formatted Name',
    'given_name': 'Test Given Name',
  };
}

test.describe('Name Employee View', () => {
  let createdId: string;
  const data = testData();

  test.beforeAll(async ({ orgContext }) => {


    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });




  test('employee can see Name list', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    const table = employeePage.locator('[data-testid="name-table"]');
    const empty = employeePage.locator('[data-testid="name-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('employee can view Name detail', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="name-field-family_name"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="name-field-formatted_name"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="name-field-given_name"]')).toBeVisible();
  });



  test('employee cannot access create form', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    await expect(employeePage.locator('[data-testid="name-create-btn"]')).toBeHidden();
  });



  test('employee cannot access edit form', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="name-edit-btn"]')).toBeHidden();
  });



  test('employee cannot delete Name', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="name-delete-btn"]')).toBeHidden();
  });

});
