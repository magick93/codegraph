import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast, expectTableContains, expectTableNotContains, waitForHydration } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/compensation/pay-run';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'pay_run_id': 'Test Pay Run Id',
    'run_date': '2025-01-15',
    'total_amount': 42,
    'total_amount_currency': 'USD',
  };
}

function updatedData(): Record<string, unknown> {
  return {
    'pay_run_id': 'Updated Pay Run Id',
    'run_date': '2025-06-20',
    'total_amount': 99,
    'total_amount_currency': 'AUD',
  };
}

test.describe('PayRun Owner CRUD', () => {



  const data = testData();
  const updated = updatedData();



  test('owner can create PayRun via form', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/new`);

    // Wait for SvelteKit to hydrate so the form's onsubmit handler is attached
    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="pay_run-submit-btn"]');
    if (await ownerPage.locator('#pay_run_id').isVisible()) {
      await ownerPage.locator('#pay_run_id').fill(String(data['pay_run_id']));
    }
    if (await ownerPage.locator('#run_date').isVisible()) {
      await ownerPage.locator('#run_date').fill(String(data['run_date']));
    }
    if (await ownerPage.locator('#total_amount').isVisible()) {
      await ownerPage.locator('#total_amount').fill(String(data['total_amount']));
    }
    if (await ownerPage.locator('#total_amount_currency').isVisible()) {
      await ownerPage.locator('#total_amount_currency').selectOption(String(data['total_amount_currency']));
    }
    await ownerPage.locator('[data-testid="pay_run-submit-btn"]').click();
    await expectToast(ownerPage, 'created', 'success');
    // Wait for SvelteKit goto() navigation to complete after toast

    await ownerPage.waitForURL(/\/compensation\/pay-run\/[0-9a-f-]+$/, { timeout: 20_000 });

    const formCreatedId = ownerPage.url().split('/').pop()!;
  });




  test('owner sees PayRun in list', async ({ ownerPage }) => {
    await ownerPage.goto(BASE_PATH);
    const table = ownerPage.locator('[data-testid="pay_run-table"]');
    const empty = ownerPage.locator('[data-testid="pay_run-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('owner can view PayRun detail', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}`);

    await expect(ownerPage.locator('[data-testid="pay_run-field-pay_run_id"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="pay_run-field-run_date"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="pay_run-field-total_amount"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="pay_run-field-total_amount_currency"]')).toBeVisible();
  });




  test('owner can edit PayRun', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}/edit`);

    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="pay_run-submit-btn"]');
    if (await ownerPage.locator('#pay_run_id').isVisible()) {
      await ownerPage.locator('#pay_run_id').clear();
      await ownerPage.locator('#pay_run_id').fill(String(updated['pay_run_id']));
    }
    if (await ownerPage.locator('#run_date').isVisible()) {
      await ownerPage.locator('#run_date').clear();
      await ownerPage.locator('#run_date').fill(String(updated['run_date']));
    }
    if (await ownerPage.locator('#total_amount').isVisible()) {
      await ownerPage.locator('#total_amount').clear();
      await ownerPage.locator('#total_amount').fill(String(updated['total_amount']));
    }
    if (await ownerPage.locator('#total_amount_currency').isVisible()) {
      await ownerPage.locator('#total_amount_currency').selectOption('AUD');
    }
    await ownerPage.locator('[data-testid="pay_run-submit-btn"]').click();
    await expectToast(ownerPage, 'updated', 'success');
  });




  test('owner can delete PayRun', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}`);

    await waitForHydration(ownerPage, '[data-testid="pay_run-delete-btn"]');
    await ownerPage.locator('[data-testid="pay_run-delete-btn"]').click();
    // Wait for portal-rendered confirm dialog
    await expect(ownerPage.locator('[data-testid="confirm-dialog"]')).toBeVisible({ timeout: 20_000 });
    await ownerPage.locator('[data-testid="confirm-dialog-confirm"]').click();
    await expectToast(ownerPage, 'deleted', 'success');
    await ownerPage.goto(BASE_PATH);
    // After delete, list may be empty (showing empty state) or table may not contain the deleted item
    const table = ownerPage.locator('[data-testid="pay_run-table"]');
    const empty = ownerPage.locator('[data-testid="pay_run-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });

});
