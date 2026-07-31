import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/common/effective-date';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'valid_from': '2025-03-10',
    'valid_to': '2025-03-10',
  };
}

test.describe('EffectiveDate Cross-Org Isolation', () => {
  let acmeEntityId: string;
  const data = testData();

  test.beforeAll(async ({ orgContext }) => {


    // Create entity as ACME owner
    const entity = await createEntityAsAcme(
      orgContext,
      BASE_PATH,
      testData(),
    );
    acmeEntityId = entity.id as string;
  });




  test('HighFive owner cannot see ACME EffectiveDate in list', async ({ highfiveOwnerPage }) => {
    await highfiveOwnerPage.goto(BASE_PATH);
    // HighFive owner should see the list page but ACME's data must not appear
    const table = highfiveOwnerPage.locator('[data-testid="effective_date-table"]');
    const empty = highfiveOwnerPage.locator('[data-testid="effective_date-empty"]');
    // Either the table is visible (without ACME data) or empty state is shown
    const tableVisible = await table.isVisible().catch(() => false);
    if (tableVisible) {
      await expect(table).not.toContainText(String(data['valid_from'] ?? ''));
    } else {
      await expect(empty).toBeVisible();
    }
  });



  test('HighFive owner cannot access ACME EffectiveDate by direct URL', async ({ highfiveOwnerPage }) => {

    const response = await highfiveOwnerPage.goto(`${BASE_PATH}/${acmeEntityId}`);

    // Should get 404, error page, or redirect — not the ACME entity data
    const content = await highfiveOwnerPage.textContent('body');
    const notFound = response?.status() === 404
      || content?.includes('Not found')
      || content?.includes('Error');
    expect(notFound).toBe(true);
  });

});
