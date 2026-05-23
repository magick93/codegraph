import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi } from '../../e2e/helpers';
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

test.describe.serial('Application Manager Team', () => {
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



  test('manager can see Application list', async ({ managerPage }) => {
    await managerPage.goto(BASE_PATH);
    const table = managerPage.locator('[data-testid="application-table"]');
    const empty = managerPage.locator('[data-testid="application-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('manager can view Application detail', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}`);

    await expect(managerPage.locator('[data-testid="application-field-application_id"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="application-field-applied_date"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="application-field-candidate_id"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="application-field-status"]')).toBeVisible();
  });



  test('manager can edit Application', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}/edit`);

    // Manager should see the edit form — verifies team-scoped write access
    await expect(managerPage.locator('[data-testid="application-form"]')).toBeVisible();
  });

});
