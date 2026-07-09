import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/recruiting/candidate';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'birth_date': '2025-03-10',
    'family_name': 'ACME Isolation Family Name',
    'given_name': 'ACME Isolation Given Name',
    // 'application_process_history': ValueObject — omit, serde default
    'candidate_id': 'ACME Isolation Candidate Id',
    'compensation_expectation': 77,
    'compensation_expectation_currency': 'USD',
    // 'distribution_guidelines': ValueObject — omit, serde default
    'external_identifier': { value: 'ACME Isolation External Identifier' },
    'gender': 'Male',
    // 'person_name': ValueObject — omit, serde default
    'position_schedule_type_codes': [{ code: 'FullTime' }],
    'position_titles': ['ACME Isolation Position Titles'],
    'qualifications': [],
    ...(depIds['referred_by_application_id_id'] ? { 'referred_by_application_id_id': depIds['referred_by_application_id_id'] } : {}),
    'status': 'active',
    'uri': 'ACME Isolation Uri',
  };
}

test.describe('Candidate Cross-Org Isolation', () => {
  let acmeEntityId: string;
  const data = testData();

  test.beforeAll(async ({ orgContext }) => {


    try {
      const dep_1 = await createEntityAsAcme(orgContext, '/recruiting/application', {  });
      depIds['referred_by_application_id_id'] = dep_1['id'] as string;
    } catch (_e) {
      // Dependency entity may already exist or have its own required fields
    }

    // Create entity as ACME owner
    const entity = await createEntityAsAcme(
      orgContext,
      BASE_PATH,
      testData(),
    );
    acmeEntityId = entity.id as string;
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



  test('HighFive owner cannot see ACME Candidate in list', async ({ highfiveOwnerPage }) => {
    await highfiveOwnerPage.goto(BASE_PATH);
    // HighFive owner should see the list page but ACME's data must not appear
    const table = highfiveOwnerPage.locator('[data-testid="candidate-table"]');
    const empty = highfiveOwnerPage.locator('[data-testid="candidate-empty"]');
    // Either the table is visible (without ACME data) or empty state is shown
    const tableVisible = await table.isVisible().catch(() => false);
    if (tableVisible) {
      await expect(table).not.toContainText(String(data['birth_date'] ?? ''));
    } else {
      await expect(empty).toBeVisible();
    }
  });



  test('HighFive owner cannot access ACME Candidate by direct URL', async ({ highfiveOwnerPage }) => {

    const response = await highfiveOwnerPage.goto(`${BASE_PATH}/${acmeEntityId}`);

    // Should get 404, error page, or redirect — not the ACME entity data
    const content = await highfiveOwnerPage.textContent('body');
    const notFound = response?.status() === 404
      || content?.includes('Not found')
      || content?.includes('Error');
    expect(notFound).toBe(true);
  });

});
