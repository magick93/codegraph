import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/events/public-event';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'is_published': true,
    'capacity': 42,
    'title': 'Test Title',
    'birth_date': '2025-01-15',
    'family_name': 'Test Family Name',
    'given_name': 'Test Given Name',
  };
}

test.describe('PublicEvent Manager Team', () => {
  let createdId: string;

  test.beforeAll(async ({ orgContext }) => {


    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });




  test('manager can see PublicEvent list', async ({ managerPage }) => {
    await managerPage.goto(BASE_PATH);
    const table = managerPage.locator('[data-testid="public_event-table"]');
    const empty = managerPage.locator('[data-testid="public_event-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('manager can view PublicEvent detail', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}`);

    await expect(managerPage.locator('[data-testid="public_event-field-is_published"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="public_event-field-capacity"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="public_event-field-title"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="public_event-field-birth_date"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="public_event-field-family_name"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="public_event-field-given_name"]')).toBeVisible();
  });



  test('manager can edit PublicEvent', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}/edit`);

    // Manager should see the edit form — verifies team-scoped write access
    await expect(managerPage.locator('[data-testid="public_event-form"]')).toBeVisible();
  });

});
