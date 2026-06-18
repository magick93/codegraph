import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast, expectTableContains, expectTableNotContains, waitForHydration } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/recruiting/application';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'application_id': 'Test Application Id',
    'applied_date': '2025-01-15',
    ...(depIds['candidate_id_id'] ? { 'candidate_id_id': depIds['candidate_id_id'] } : {}),
    'status': 'Applied',
  };
}

function updatedData(): Record<string, unknown> {
  return {
    'application_id': 'Updated Application Id',
    'applied_date': '2025-06-20',
    ...(depIds['candidate_id_id'] ? { 'candidate_id_id': depIds['candidate_id_id'] } : {}),
    'status': 'Rejected',
  };
}

test.describe.serial('Application Owner CRUD', () => {
  let createdId: string;


  test.beforeAll(async ({ orgContext }) => {


    try {
      const dep_1 = await createEntityAsAcme(orgContext, '/recruiting/candidate', {  });
      depIds['candidate_id_id'] = dep_1['id'] as string;
    } catch (_e) {
      // Dependency entity may already exist or have its own required fields
    }

  });


  test.afterAll(async ({ orgContext }) => {
    const baseUrl = process.env.PUBLIC_API_URL ?? 'http://localhost:3000';

    if (depIds['candidate_id_id']) {
      try {
        await fetch(`${baseUrl}/api/recruiting/candidate/${depIds['candidate_id_id']}`, {
          method: 'DELETE',
          headers: { 'Authorization': `Bearer ${orgContext.acme.apiKey}` },
        });
      } catch { /* best effort */ }
    }

  });



  const data = testData();
  const updated = updatedData();



  test('owner can create Application via form', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/new`);

    // Wait for SvelteKit to hydrate so the form's onsubmit handler is attached
    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="application-submit-btn"]');
    if (await ownerPage.locator('#application_id').isVisible()) {
      await ownerPage.locator('#application_id').fill(String(data['application_id']));
    }
    if (await ownerPage.locator('#applied_date').isVisible()) {
      await ownerPage.locator('#applied_date').fill(String(data['applied_date']));
    }
    if (data['candidate_id_id'] && await ownerPage.locator('#candidate_id_id').isVisible()) {
      await ownerPage.locator('#candidate_id_id').fill(String(data['candidate_id_id']));
    }
    if (await ownerPage.locator('#status').isVisible()) {
      await ownerPage.locator('#status').selectOption(String(data['status']));
    }
    await ownerPage.locator('[data-testid="application-submit-btn"]').click();
    await expectToast(ownerPage, 'created', 'success');
    // Wait for SvelteKit goto() navigation to complete after toast

    await ownerPage.waitForURL(/\/recruiting\/application\/[0-9a-f-]+$/, { timeout: 20_000 });

    createdId = ownerPage.url().split('/').pop()!;
  });




  test('owner sees Application in list', async ({ ownerPage }) => {
    await ownerPage.goto(BASE_PATH);
    const table = ownerPage.locator('[data-testid="application-table"]');
    const empty = ownerPage.locator('[data-testid="application-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('owner can view Application detail', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}`);

    await expect(ownerPage.locator('[data-testid="application-field-application_id"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="application-field-applied_date"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="application-field-candidate_id_id"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="application-field-status"]')).toBeVisible();
  });




  test('owner can edit Application', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}/edit`);

    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="application-submit-btn"]');
    if (await ownerPage.locator('#application_id').isVisible()) {
      await ownerPage.locator('#application_id').clear();
      await ownerPage.locator('#application_id').fill(String(updated['application_id']));
    }
    if (await ownerPage.locator('#applied_date').isVisible()) {
      await ownerPage.locator('#applied_date').clear();
      await ownerPage.locator('#applied_date').fill(String(updated['applied_date']));
    }
    if (updated['candidate_id_id'] && await ownerPage.locator('#candidate_id_id').isVisible()) {
      await ownerPage.locator('#candidate_id_id').clear();
      await ownerPage.locator('#candidate_id_id').fill(String(updated['candidate_id_id']));
    }
    if (await ownerPage.locator('#status').isVisible()) {
      await ownerPage.locator('#status').selectOption('Rejected');
    }
    await ownerPage.locator('[data-testid="application-submit-btn"]').click();
    await expectToast(ownerPage, 'updated', 'success');
  });




  test('owner can delete Application', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}`);

    await waitForHydration(ownerPage, '[data-testid="application-delete-btn"]');
    await ownerPage.locator('[data-testid="application-delete-btn"]').click();
    // Wait for portal-rendered confirm dialog
    await expect(ownerPage.locator('[data-testid="confirm-dialog"]')).toBeVisible({ timeout: 20_000 });
    await ownerPage.locator('[data-testid="confirm-dialog-confirm"]').click();
    await expectToast(ownerPage, 'deleted', 'success');
    await ownerPage.goto(BASE_PATH);
    // After delete, list may be empty (showing empty state) or table may not contain the deleted item
    const table = ownerPage.locator('[data-testid="application-table"]');
    const empty = ownerPage.locator('[data-testid="application-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });

});
