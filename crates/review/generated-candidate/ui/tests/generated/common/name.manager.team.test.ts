import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi } from '../../e2e/helpers';
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

test.describe.serial('Name Manager Team', () => {
  let createdId: string;
  const data = testData();

  test.beforeAll(async ({ orgContext }) => {


    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });




  test('manager can see Name list', async ({ managerPage }) => {
    await managerPage.goto(BASE_PATH);
    const table = managerPage.locator('[data-testid="name-table"]');
    const empty = managerPage.locator('[data-testid="name-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('manager can view Name detail', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}`);

    await expect(managerPage.locator('[data-testid="name-field-family_name"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="name-field-formatted_name"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="name-field-given_name"]')).toBeVisible();
  });



  test('manager can edit Name', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}/edit`);

    // Manager should see the edit form — verifies team-scoped write access
    await expect(managerPage.locator('[data-testid="name-form"]')).toBeVisible();
  });

});
