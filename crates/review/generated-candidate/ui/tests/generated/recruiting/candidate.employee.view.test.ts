import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/recruiting/candidate';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'birth_date': '2025-01-15',
    'family_name': 'Test Family Name',
    'given_name': 'Test Given Name',
    'candidate_id': 'Test Candidate Id',
    'compensation_expectation': 42,
    'compensation_expectation_currency': 'USD',
    'external_identifier': { value: 'Test External Identifier' },
    'gender': 'Male',
    'position_schedule_type_codes': [{ code: 'FullTime' }],
    'position_titles': ['Test Position Titles'],
    ...(depIds['referred_by_application_id'] ? { 'referred_by_application_id': depIds['referred_by_application_id'] } : {}),
    'status': 'active',
    'uri': 'Test Uri',
  };
}

test.describe.serial('Candidate Employee View', () => {
  let createdId: string;
  const data = testData();

  test.beforeAll(async ({ orgContext }) => {


    try {
      const dep_1 = await createEntityAsAcme(orgContext, '/recruiting/application', { 'applied_date': '2025-01-15', 'status': 'Applied' });
      depIds['referred_by_application_id'] = dep_1['id'] as string;
    } catch (_e) {
      // Dependency entity may already exist or have its own required fields
    }

    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });


  test.afterAll(async ({ orgContext }) => {
    const baseUrl = process.env.PUBLIC_API_URL ?? 'http://localhost:3000';

    if (depIds['referred_by_application_id']) {
      try {
        await fetch(`${baseUrl}/api/recruiting/application/${depIds['referred_by_application_id']}`, {
          method: 'DELETE',
          headers: { 'Authorization': `Bearer ${orgContext.acme.apiKey}` },
        });
      } catch { /* best effort */ }
    }

  });



  test('employee can see Candidate list', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    const table = employeePage.locator('[data-testid="candidate-table"]');
    const empty = employeePage.locator('[data-testid="candidate-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('employee can view Candidate detail', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="candidate-field-birth_date"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="candidate-field-family_name"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="candidate-field-given_name"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="candidate-field-candidate_id"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="candidate-field-compensation_expectation"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="candidate-field-compensation_expectation_currency"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="candidate-field-external_identifier"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="candidate-field-gender"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="candidate-field-position_schedule_type_codes"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="candidate-field-position_titles"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="candidate-field-referred_by_application_id"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="candidate-field-status"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="candidate-field-uri"]')).toBeVisible();
  });



  test('employee cannot access create form', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    await expect(employeePage.locator('[data-testid="candidate-create-btn"]')).toBeHidden();
  });



  test('employee cannot access edit form', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="candidate-edit-btn"]')).toBeHidden();
  });



});
