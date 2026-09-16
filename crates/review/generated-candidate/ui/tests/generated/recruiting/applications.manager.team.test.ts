import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi } from '../../e2e/helpers';
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

test.describe('Application Manager Team', () => {
  let createdId: string;

  test.beforeAll(async ({ orgContext }) => {




    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
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
