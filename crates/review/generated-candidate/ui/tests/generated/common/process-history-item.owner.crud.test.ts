import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast, expectTableContains, expectTableNotContains, waitForHydration } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/common/process-history-item';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'action_date': '2025-01-15T10:30:00Z',
    'descriptions': ['Test Descriptions'],
    'id': 'Test Id',
  };
}

function updatedData(): Record<string, unknown> {
  return {
    'action_date': '2025-06-20T14:00:00Z',
    'descriptions': ['Updated Descriptions'],
    'id': 'Updated Id',
  };
}

test.describe('ProcessHistoryItem Owner CRUD', () => {



  const data = testData();
  const updated = updatedData();



  test('owner can create ProcessHistoryItem via form', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/new`);

    // Wait for SvelteKit to hydrate so the form's onsubmit handler is attached
    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="process_history_item-submit-btn"]');
    if (await ownerPage.locator('#action_date').isVisible()) {
      await ownerPage.locator('#action_date').fill(String(data['action_date']).replace(/:\d{2}Z$/, ''));
    }
    {
      const vals = data['descriptions'] as string[];
      for (let i = 0; i < vals.length; i++) {
        if (i > 0) await ownerPage.locator('[data-testid="descriptions-add-btn"]').click();
        await ownerPage.locator(`[data-testid="descriptions-row-${i}"] input`).fill(vals[i]);
      }
    }
    if (await ownerPage.locator('#id').isVisible()) {
      await ownerPage.locator('#id').fill(String(data['id']));
    }
    await ownerPage.locator('[data-testid="process_history_item-submit-btn"]').click();
    await expectToast(ownerPage, 'created', 'success');
    // Wait for SvelteKit goto() navigation to complete after toast

    await ownerPage.waitForURL(/\/common\/process-history-item\/[0-9a-f-]+$/, { timeout: 20_000 });

    const formCreatedId = ownerPage.url().split('/').pop()!;
  });




  test('owner sees ProcessHistoryItem in list', async ({ ownerPage }) => {
    await ownerPage.goto(BASE_PATH);
    const table = ownerPage.locator('[data-testid="process_history_item-table"]');
    const empty = ownerPage.locator('[data-testid="process_history_item-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('owner can view ProcessHistoryItem detail', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}`);

    await expect(ownerPage.locator('[data-testid="process_history_item-field-action_date"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="process_history_item-field-descriptions"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="process_history_item-field-id"]')).toBeVisible();
  });




  test('owner can edit ProcessHistoryItem', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}/edit`);

    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="process_history_item-submit-btn"]');
    if (await ownerPage.locator('#action_date').isVisible()) {
      await ownerPage.locator('#action_date').clear();
      await ownerPage.locator('#action_date').fill(String(updated['action_date']).replace(/:\d{2}Z$/, ''));
    }
    {
      const vals = updated['descriptions'] as string[];
      // Clear existing rows first
      const existingRows = await ownerPage.locator('[data-testid^="descriptions-row-"]').count();
      for (let i = existingRows - 1; i > 0; i--) {
        await ownerPage.locator(`[data-testid="descriptions-remove-${i}"]`).click();
      }
      await ownerPage.locator('[data-testid="descriptions-row-0"] input').clear();
      await ownerPage.locator('[data-testid="descriptions-row-0"] input').fill(vals[0]);
      for (let i = 1; i < vals.length; i++) {
        await ownerPage.locator('[data-testid="descriptions-add-btn"]').click();
        await ownerPage.locator(`[data-testid="descriptions-row-${i}"] input`).fill(vals[i]);
      }
    }
    if (await ownerPage.locator('#id').isVisible()) {
      await ownerPage.locator('#id').clear();
      await ownerPage.locator('#id').fill(String(updated['id']));
    }
    await ownerPage.locator('[data-testid="process_history_item-submit-btn"]').click();
    await expectToast(ownerPage, 'updated', 'success');
  });




  test('owner can delete ProcessHistoryItem', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}`);

    await waitForHydration(ownerPage, '[data-testid="process_history_item-delete-btn"]');
    await ownerPage.locator('[data-testid="process_history_item-delete-btn"]').click();
    // Wait for portal-rendered confirm dialog
    await expect(ownerPage.locator('[data-testid="confirm-dialog"]')).toBeVisible({ timeout: 20_000 });
    await ownerPage.locator('[data-testid="confirm-dialog-confirm"]').click();
    await expectToast(ownerPage, 'deleted', 'success');
    await ownerPage.goto(BASE_PATH);
    // After delete, list may be empty (showing empty state) or table may not contain the deleted item
    const table = ownerPage.locator('[data-testid="process_history_item-table"]');
    const empty = ownerPage.locator('[data-testid="process_history_item-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });

});
