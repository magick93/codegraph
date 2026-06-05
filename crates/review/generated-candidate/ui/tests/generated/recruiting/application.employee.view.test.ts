import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/recruiting/application';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'application_id': 'Test Application Id',
    'applied_date': '2025-01-15',
    ...(depIds['candidate_id'] ? { 'candidate_id': depIds['candidate_id'] } : {}),
    'status': 'Applied',
  };
}

test.describe.serial('Application Employee View', () => {
  let createdId: string;
  const data = testData();

  test.beforeAll(async ({ orgContext }) => {


    try {
      const dep_1 = await createEntityAsAcme(orgContext, '/recruiting/candidate', { 'birth_date': '2025-01-15', 'family_name': 'Test Family Name', 'given_name': 'Test Given Name', 'compensation_expectation': 42, 'compensation_expectation_currency': 'USD', 'gender': 'Male', 'position_schedule_type_codes': [{ code: 'FullTime' }], 'position_titles': ['Test Position Titles'], 'status': 'active', 'uri': 'Test Uri' });
      depIds['candidate_id'] = dep_1['id'] as string;
    } catch (_e) {
      // Dependency entity may already exist or have its own required fields
    }

    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });


  test.afterAll(async ({ orgContext }) => {
    const baseUrl = process.env.PUBLIC_API_URL ?? 'http://localhost:3000';

    if (depIds['candidate_id']) {
      try {
        await fetch(`${baseUrl}/api/recruiting/candidate/${depIds['candidate_id']}`, {
          method: 'DELETE',
          headers: { 'Authorization': `Bearer ${orgContext.acme.apiKey}` },
        });
      } catch { /* best effort */ }
    }

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
