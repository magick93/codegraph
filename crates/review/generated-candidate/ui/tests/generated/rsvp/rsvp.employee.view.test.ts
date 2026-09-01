import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast } from '../../e2e/helpers';
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

test.describe('Rsvp Employee View', () => {
  let createdId: string;

  test.beforeAll(async ({ orgContext }) => {


    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });




  test('employee can see Rsvp list', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    const table = employeePage.locator('[data-testid="rsvp-table"]');
    const empty = employeePage.locator('[data-testid="rsvp-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('employee can view Rsvp detail', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="rsvp-field-event"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="rsvp-field-status"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="rsvp-field-timestamp"]')).toBeVisible();
  });



  test('employee cannot access create form', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    await expect(employeePage.locator('[data-testid="rsvp-create-btn"]')).toBeHidden();
  });



  test('employee cannot access edit form', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="rsvp-edit-btn"]')).toBeHidden();
  });



  test('employee cannot delete Rsvp', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="rsvp-delete-btn"]')).toBeHidden();
  });

});
