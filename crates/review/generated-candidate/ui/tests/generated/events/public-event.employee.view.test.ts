import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast } from '../../e2e/helpers';
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

test.describe('PublicEvent Employee View', () => {
  let createdId: string;

  test.beforeAll(async ({ orgContext }) => {


    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });




  test('employee can see PublicEvent list', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    const table = employeePage.locator('[data-testid="public_event-table"]');
    const empty = employeePage.locator('[data-testid="public_event-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('employee can view PublicEvent detail', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="public_event-field-is_published"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="public_event-field-capacity"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="public_event-field-title"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="public_event-field-birth_date"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="public_event-field-family_name"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="public_event-field-given_name"]')).toBeVisible();
  });



  test('employee cannot access create form', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    await expect(employeePage.locator('[data-testid="public_event-create-btn"]')).toBeHidden();
  });



  test('employee cannot access edit form', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="public_event-edit-btn"]')).toBeHidden();
  });



  test('employee cannot delete PublicEvent', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="public_event-delete-btn"]')).toBeHidden();
  });

});
