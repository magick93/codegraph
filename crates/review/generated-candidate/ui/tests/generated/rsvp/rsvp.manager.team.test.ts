import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/rsvp/rsvp';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    // 'event': ValueObject — omit, serde default
    'status': 'Confirmed',
    'timestamp': '2025-01-15T10:30:00Z',
  };
}

test.describe('Rsvp Manager Team', () => {
  let createdId: string;

  test.beforeAll(async ({ orgContext }) => {


    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });




  test('manager can see Rsvp list', async ({ managerPage }) => {
    await managerPage.goto(BASE_PATH);
    const table = managerPage.locator('[data-testid="rsvp-table"]');
    const empty = managerPage.locator('[data-testid="rsvp-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('manager can view Rsvp detail', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}`);

    await expect(managerPage.locator('[data-testid="rsvp-field-event"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="rsvp-field-status"]')).toBeVisible();
    await expect(managerPage.locator('[data-testid="rsvp-field-timestamp"]')).toBeVisible();
  });



  test('manager can edit Rsvp', async ({ managerPage }) => {

    await managerPage.goto(`${BASE_PATH}/${createdId}/edit`);

    // Manager should see the edit form — verifies team-scoped write access
    await expect(managerPage.locator('[data-testid="rsvp-form"]')).toBeVisible();
  });

});
