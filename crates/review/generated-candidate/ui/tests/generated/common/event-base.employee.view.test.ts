import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast } from '../../e2e/helpers';
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

test.describe('EventBase Employee View', () => {
  let createdId: string;

  test.beforeAll(async ({ orgContext }) => {


    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });




  test('employee can see EventBase list', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    const table = employeePage.locator('[data-testid="event_base-table"]');
    const empty = employeePage.locator('[data-testid="event_base-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('employee can view EventBase detail', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="event_base-field-capacity"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="event_base-field-title"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="event_base-field-birth_date"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="event_base-field-family_name"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="event_base-field-given_name"]')).toBeVisible();
  });



  test('employee cannot access create form', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    await expect(employeePage.locator('[data-testid="event_base-create-btn"]')).toBeHidden();
  });



  test('employee cannot access edit form', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="event_base-edit-btn"]')).toBeHidden();
  });



  test('employee cannot delete EventBase', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="event_base-delete-btn"]')).toBeHidden();
  });

});
