import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast, expectTableContains, expectTableNotContains, waitForHydration } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';


const BASE_PATH = '/common/person-base';


// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'birth_date': '2025-01-15',
    'family_name': 'Test Family Name',
    'given_name': 'Test Given Name',
  };
}

function updatedData(): Record<string, unknown> {
  return {
    'birth_date': '2025-06-20',
    'family_name': 'Updated Family Name',
    'given_name': 'Updated Given Name',
  };
}

test.describe('PersonBase Owner CRUD', () => {



  const data = testData();
  const updated = updatedData();



  test('owner can create PersonBase via form', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/new`);

    // Wait for SvelteKit to hydrate so the form's onsubmit handler is attached
    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="person_base-submit-btn"]');
    if (await ownerPage.locator('#birth_date').isVisible()) {
      await ownerPage.locator('#birth_date').fill(String(data['birth_date']));
    }
    if (await ownerPage.locator('#family_name').isVisible()) {
      await ownerPage.locator('#family_name').fill(String(data['family_name']));
    }
    if (await ownerPage.locator('#given_name').isVisible()) {
      await ownerPage.locator('#given_name').fill(String(data['given_name']));
    }
    await ownerPage.locator('[data-testid="person_base-submit-btn"]').click();
    await expectToast(ownerPage, 'created', 'success');
    // Wait for SvelteKit goto() navigation to complete after toast

    await ownerPage.waitForURL(/\/common\/person-base\/[0-9a-f-]+$/, { timeout: 20_000 });

    const formCreatedId = ownerPage.url().split('/').pop()!;
  });




  test('owner sees PersonBase in list', async ({ ownerPage }) => {
    await ownerPage.goto(BASE_PATH);
    const table = ownerPage.locator('[data-testid="person_base-table"]');
    const empty = ownerPage.locator('[data-testid="person_base-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('owner can view PersonBase detail', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}`);

    await expect(ownerPage.locator('[data-testid="person_base-field-birth_date"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="person_base-field-family_name"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="person_base-field-given_name"]')).toBeVisible();
  });




  test('owner can edit PersonBase', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}/edit`);

    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="person_base-submit-btn"]');
    if (await ownerPage.locator('#birth_date').isVisible()) {
      await ownerPage.locator('#birth_date').clear();
      await ownerPage.locator('#birth_date').fill(String(updated['birth_date']));
    }
    if (await ownerPage.locator('#family_name').isVisible()) {
      await ownerPage.locator('#family_name').clear();
      await ownerPage.locator('#family_name').fill(String(updated['family_name']));
    }
    if (await ownerPage.locator('#given_name').isVisible()) {
      await ownerPage.locator('#given_name').clear();
      await ownerPage.locator('#given_name').fill(String(updated['given_name']));
    }
    await ownerPage.locator('[data-testid="person_base-submit-btn"]').click();
    await expectToast(ownerPage, 'updated', 'success');
  });




  test('owner can delete PersonBase', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}`);

    await waitForHydration(ownerPage, '[data-testid="person_base-delete-btn"]');
    await ownerPage.locator('[data-testid="person_base-delete-btn"]').click();
    // Wait for portal-rendered confirm dialog
    await expect(ownerPage.locator('[data-testid="confirm-dialog"]')).toBeVisible({ timeout: 20_000 });
    await ownerPage.locator('[data-testid="confirm-dialog-confirm"]').click();
    await expectToast(ownerPage, 'deleted', 'success');
    await ownerPage.goto(BASE_PATH);
    // After delete, list may be empty (showing empty state) or table may not contain the deleted item
    const table = ownerPage.locator('[data-testid="person_base-table"]');
    const empty = ownerPage.locator('[data-testid="person_base-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });

});
