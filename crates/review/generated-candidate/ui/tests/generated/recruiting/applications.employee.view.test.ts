import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/recruiting/applications';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string | string[]> = {};


function testData(): Record<string, unknown> {
  return {
    'application_id': 'Test Application Id',
    'applied_date': '2025-01-15',
    // 'candidate_id': entity ref — emitted by _dep_setup.tera
    'status': 'Applied',


  };
}

test.describe('Application Employee View', () => {
  let createdId: string;

  test.beforeAll(async ({ orgContext }) => {




    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
});




  test('employee can see Application list', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    const table = employeePage.locator('[data-testid="application-table"]');
    const empty = employeePage.locator('[data-testid="application-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });

  test('employee can view Application detail', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="application-field-application_id"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="application-field-applied_date"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="application-field-candidate_id"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="application-field-status"]')).toBeVisible();
  });

  test('employee cannot access create form', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    await expect(employeePage.locator('[data-testid="application-create-btn"]')).toBeHidden();
  });

  test('employee cannot access edit form', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="application-edit-btn"]')).toBeHidden();
  });

  test('employee cannot delete Application', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="application-delete-btn"]')).toBeHidden();
  });
});
