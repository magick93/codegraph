import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/common/event-base';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'capacity': 42,
    'title': 'Test Title',
    'birth_date': '2025-01-15',
    'family_name': 'Test Family Name',
    'given_name': 'Test Given Name',
  };
}

test.describe('EventBase Manager Team', () => {
  let createdId: string;

  test.beforeAll(async ({ orgContext }) => {


    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });




  test('manager can see EventBase list', async ({ managerPage }) => {
    await managerPage.goto(BASE_PATH);
    const table = managerPage.locator('[data-testid="event_base-table"]');
    const empty = managerPage.locator('[data-testid="event_base-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('manager can view EventBase detail', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}`);

    await expect(managerPage.locator('[data-testid="event_base-field-capacity"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="event_base-field-title"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="event_base-field-birth_date"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="event_base-field-family_name"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="event_base-field-given_name"]')).toBeVisible();
  });



  test('manager can edit EventBase', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}/edit`);

    // Manager should see the edit form — verifies team-scoped write access
    await expect(managerPage.locator('[data-testid="event_base-form"]')).toBeVisible();
  });

});
