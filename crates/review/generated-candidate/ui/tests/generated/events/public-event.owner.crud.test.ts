import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast, expectTableContains, expectTableNotContains, waitForHydration } from '../../e2e/helpers';
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

function updatedData(): Record<string, unknown> {
  return {
    'is_published': false,
    'capacity': 99,
    'title': 'Updated Title',
    'birth_date': '2025-06-20',
    'family_name': 'Updated Family Name',
    'given_name': 'Updated Given Name',
  };
}

test.describe('PublicEvent Owner CRUD', () => {



  let data: Record<string, unknown>;
  let updated: Record<string, unknown>;
  test.beforeEach(() => {
    data = testData();
    updated = updatedData();
  });



  test('owner can create PublicEvent via form', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/new`);

    // Wait for SvelteKit to hydrate so the form's onsubmit handler is attached
    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="public_event-submit-btn"]');
    if (data['is_published'] && await ownerPage.locator('#is_published').isVisible()) {
      await ownerPage.locator('#is_published').click();
    }
    if (await ownerPage.locator('#capacity').isVisible()) {
      await ownerPage.locator('#capacity').fill(String(data['capacity']));
    }
    if (await ownerPage.locator('#title').isVisible()) {
      await ownerPage.locator('#title').fill(String(data['title']));
    }
    if (await ownerPage.locator('#birth_date').isVisible()) {
      await ownerPage.locator('#birth_date').fill(String(data['birth_date']));
    }
    if (await ownerPage.locator('#family_name').isVisible()) {
      await ownerPage.locator('#family_name').fill(String(data['family_name']));
    }
    if (await ownerPage.locator('#given_name').isVisible()) {
      await ownerPage.locator('#given_name').fill(String(data['given_name']));
    }
    await ownerPage.locator('[data-testid="public_event-submit-btn"]').click();
    await expectToast(ownerPage, 'created', 'success');
    // Wait for SvelteKit goto() navigation to complete after toast

    await ownerPage.waitForURL(/\/events\/public-event\/[0-9a-f-]+$/, { timeout: 20_000 });

    const formCreatedId = ownerPage.url().split('/').pop()!;
  });




  test('owner sees PublicEvent in list', async ({ ownerPage }) => {
    await ownerPage.goto(BASE_PATH);
    const table = ownerPage.locator('[data-testid="public_event-table"]');
    const empty = ownerPage.locator('[data-testid="public_event-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('owner can view PublicEvent detail', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}`);

    await expect(ownerPage.locator('[data-testid="public_event-field-is_published"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="public_event-field-capacity"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="public_event-field-title"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="public_event-field-birth_date"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="public_event-field-family_name"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="public_event-field-given_name"]')).toBeVisible();
  });




  test('owner can edit PublicEvent', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}/edit`);

    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="public_event-submit-btn"]');
    if (await ownerPage.locator('#is_published').isVisible()) {
      const isChecked = await ownerPage.locator('#is_published').getAttribute('aria-checked') === 'true';
      if (Boolean(updated['is_published']) !== isChecked) {
        await ownerPage.locator('#is_published').click();
      }
    }
    if (await ownerPage.locator('#capacity').isVisible()) {
      await ownerPage.locator('#capacity').clear();
      await ownerPage.locator('#capacity').fill(String(updated['capacity']));
    }
    if (await ownerPage.locator('#title').isVisible()) {
      await ownerPage.locator('#title').clear();
      await ownerPage.locator('#title').fill(String(updated['title']));
    }
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
    await ownerPage.locator('[data-testid="public_event-submit-btn"]').click();
    await expectToast(ownerPage, 'updated', 'success');
  });




  test('owner can delete PublicEvent', async ({ ownerPage, orgContext }) => {
    const entity = await createEntityAsAcme(orgContext, BASE_PATH, testData());
    const myId = entity['id'] as string;

    await ownerPage.goto(`${BASE_PATH}/${myId}`);

    await waitForHydration(ownerPage, '[data-testid="public_event-delete-btn"]');
    await ownerPage.locator('[data-testid="public_event-delete-btn"]').click();
    // Wait for portal-rendered confirm dialog
    await expect(ownerPage.locator('[data-testid="confirm-dialog"]')).toBeVisible({ timeout: 20_000 });
    await ownerPage.locator('[data-testid="confirm-dialog-confirm"]').click();
    await expectToast(ownerPage, 'deleted', 'success');
    await ownerPage.goto(BASE_PATH);
    // After delete, list may be empty (showing empty state) or table may not contain the deleted item
    const table = ownerPage.locator('[data-testid="public_event-table"]');
    const empty = ownerPage.locator('[data-testid="public_event-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });

});
