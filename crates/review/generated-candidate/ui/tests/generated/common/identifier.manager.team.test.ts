import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi } from '../../e2e/helpers';
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

test.describe.serial('Identifier Manager Team', () => {
  let createdId: string;
  const data = testData();

  test.beforeAll(async ({ orgContext }) => {


    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });




  test('manager can see Identifier list', async ({ managerPage }) => {
    await managerPage.goto(BASE_PATH);
    const table = managerPage.locator('[data-testid="identifier-table"]');
    const empty = managerPage.locator('[data-testid="identifier-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('manager can view Identifier detail', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}`);

    await expect(managerPage.locator('[data-testid="identifier-field-scheme_agency_id"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="identifier-field-scheme_id"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="identifier-field-scheme_version_id"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="identifier-field-value"]')).toBeVisible();
  });



  test('manager can edit Identifier', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}/edit`);

    // Manager should see the edit form — verifies team-scoped write access
    await expect(managerPage.locator('[data-testid="identifier-form"]')).toBeVisible();
  });

});
