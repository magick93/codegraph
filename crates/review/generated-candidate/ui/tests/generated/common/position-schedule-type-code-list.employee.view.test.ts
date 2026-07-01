import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/common/position-schedule-type-code-list';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'code': `TestCode-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
    'display_name': 'Test Display Name',
    'sort_order': 42,
  };
}

test.describe.serial('PositionScheduleTypeCodeList Employee View', () => {
  let createdId: string;
  const data = testData();

  test.beforeAll(async ({ orgContext }) => {


    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    createdId = entity.id as string;
  });




  test('employee can see PositionScheduleTypeCodeList list', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    const table = employeePage.locator('[data-testid="position_schedule_type_code_list-table"]');
    const empty = employeePage.locator('[data-testid="position_schedule_type_code_list-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('employee can view PositionScheduleTypeCodeList detail', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="position_schedule_type_code_list-field-code"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="position_schedule_type_code_list-field-display_name"]')).toBeVisible();
    await expect(employeePage.locator('[data-testid="position_schedule_type_code_list-field-sort_order"]')).toBeVisible();
  });



  test('employee cannot access create form', async ({ employeePage }) => {
    await employeePage.goto(BASE_PATH);
    await expect(employeePage.locator('[data-testid="position_schedule_type_code_list-create-btn"]')).toBeHidden();
  });



  test('employee cannot access edit form', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="position_schedule_type_code_list-edit-btn"]')).toBeHidden();
  });



  test('employee cannot delete PositionScheduleTypeCodeList', async ({ employeePage }) => {

    await employeePage.goto(`${BASE_PATH}/${createdId}`);

    await expect(employeePage.locator('[data-testid="position_schedule_type_code_list-delete-btn"]')).toBeHidden();
  });

});
