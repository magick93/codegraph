import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/recruiting/candidate';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'birth_date': '2025-01-15',
    'family_name': 'Test Family Name',
    'given_name': 'Test Given Name',
    'application_process_history': 'Test Application Process History',
    'candidate_id': 'Test Candidate Id',
    'compensation_expectation': 42,
    'compensation_expectation_currency': 'USD',
    'distribution_guidelines': 'Test Distribution Guidelines',
    'external_identifier': { value: 'Test External Identifier' },
    'gender': 'Male',
    'person_name': 'Test Person Name',
    'position_schedule_type_codes': [{ code: 'FullTime' }],
    'position_titles': ['Test Position Titles'],
    'qualifications': ['Test Qualifications'],
    ...(depIds['referred_by_application_id_id'] ? { 'referred_by_application_id_id': depIds['referred_by_application_id_id'] } : {}),
    'status': 'active',
    'uri': 'Test Uri',
  };
}

test.describe.serial('Candidate Manager Team', () => {
  let createdId: string;
  const data = testData();

  test.beforeAll(async ({ orgContext }) => {


    try {
      const dep_1 = await createEntityAsAcme(orgContext, '/recruiting/application', {  });
      depIds['referred_by_application_id_id'] = dep_1['id'] as string;
    } catch (_e) {
      // Dependency entity may already exist or have its own required fields
    }

    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });


  test.afterAll(async ({ orgContext }) => {
    const baseUrl = process.env.PUBLIC_API_URL ?? 'http://localhost:3000';

    if (depIds['referred_by_application_id_id']) {
      try {
        await fetch(`${baseUrl}/api/recruiting/application/${depIds['referred_by_application_id_id']}`, {
          method: 'DELETE',
          headers: { 'Authorization': `Bearer ${orgContext.acme.apiKey}` },
        });
      } catch { /* best effort */ }
    }

  });



  test('manager can see Candidate list', async ({ managerPage }) => {
    await managerPage.goto(BASE_PATH);
    const table = managerPage.locator('[data-testid="candidate-table"]');
    const empty = managerPage.locator('[data-testid="candidate-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('manager can view Candidate detail', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}`);

    await expect(managerPage.locator('[data-testid="candidate-field-birth_date"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="candidate-field-family_name"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="candidate-field-given_name"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="candidate-field-application_process_history"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="candidate-field-candidate_id"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="candidate-field-compensation_expectation"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="candidate-field-compensation_expectation_currency"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="candidate-field-distribution_guidelines"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="candidate-field-external_identifier"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="candidate-field-gender"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="candidate-field-person_name"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="candidate-field-position_schedule_type_codes"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="candidate-field-position_titles"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="candidate-field-qualifications"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="candidate-field-referred_by_application_id_id"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="candidate-field-status"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="candidate-field-uri"]')).toBeVisible();
  });



  test('manager can edit Candidate', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}/edit`);

    // Manager should see the edit form — verifies team-scoped write access
    await expect(managerPage.locator('[data-testid="candidate-form"]')).toBeVisible();
  });

});
