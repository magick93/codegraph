import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/recruiting/application';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'application_id': 'ACME Isolation Application Id',
    'applied_date': '2025-03-10',
    ...(depIds['candidate_id'] ? { 'candidate_id': depIds['candidate_id'] } : {}),
    'status': 'Applied',
  };
}

test.describe.serial('Application Cross-Org Isolation', () => {
  let acmeEntityId: string;
  const data = testData();

  test.beforeAll(async ({ orgContext }) => {


    try {
      const dep_1 = await createEntityAsAcme(orgContext, '/recruiting/candidate', { 'birth_date': '2025-01-15', 'family_name': 'Test Family Name', 'given_name': 'Test Given Name', 'compensation_expectation': 42, 'compensation_expectation_currency': 'USD', 'external_identifier': { value: 'Test External Identifier' }, 'gender': 'Male', 'position_schedule_type_codes': [{ code: 'FullTime' }], 'position_titles': ['Test Position Titles'], 'status': 'active', 'uri': 'Test Uri' });
      depIds['candidate_id'] = dep_1['id'] as string;
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

    if (depIds['candidate_id']) {
      try {
        await fetch(`${baseUrl}/api/recruiting/candidate/${depIds['candidate_id']}`, {
          method: 'DELETE',
          headers: { 'Authorization': `Bearer ${orgContext.acme.apiKey}` },
        });
      } catch { /* best effort */ }
    }

  });



  test('HighFive owner cannot see ACME Application in list', async ({ highfiveOwnerPage }) => {
    await highfiveOwnerPage.goto(BASE_PATH);
    // HighFive owner should see the list page but ACME's data must not appear
    const table = highfiveOwnerPage.locator('[data-testid="application-table"]');
    const empty = highfiveOwnerPage.locator('[data-testid="application-empty"]');
    // Either the table is visible (without ACME data) or empty state is shown
    const tableVisible = await table.isVisible().catch(() => false);
    if (tableVisible) {
      await expect(table).not.toContainText(String(data['application_id'] ?? ''));
    } else {
      await expect(empty).toBeVisible();
    }
  });



  test('HighFive owner cannot access ACME Application by direct URL', async ({ highfiveOwnerPage }) => {

    const response = await highfiveOwnerPage.goto(`${BASE_PATH}/${acmeEntityId}`);

    // Should get 404, error page, or redirect — not the ACME entity data
    const content = await highfiveOwnerPage.textContent('body');
    const notFound = response?.status() === 404
      || content?.includes('Not found')
      || content?.includes('Error');
    expect(notFound).toBe(true);
  });

});
