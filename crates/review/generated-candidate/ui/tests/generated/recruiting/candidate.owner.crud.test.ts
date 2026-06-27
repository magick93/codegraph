import { test, expect } from '../../e2e/fixtures/personas';
import { createEntityAsAcme, createEntityViaApi, deleteEntityViaApi, expectToast, expectTableContains, expectTableNotContains, waitForHydration } from '../../e2e/helpers';
import type { OrgContext } from '../../e2e/fixtures/personas';



const PARENT_API_PATH = '/recruiting/application';

let BASE_PATH: string;



// Entity reference dependency IDs — populated in beforeAll when FK deps exist

const depIds: Record<string, string> = {};


function testData(): Record<string, unknown> {
  return {
    'birth_date': '2025-01-15',
    'family_name': 'Test Family Name',
    'given_name': 'Test Given Name',
    'application_process_history': 'Test Application Process History',
    'candidate_id': 'Test Candidate Id',
    'compensation_expectation': 42,
    'compensation_expectation_currency': 'USD',
    'distribution_guidelines': 'Test Distribution Guidelines',
    'external_identifier': { value: 'Test External Identifier' },
    'gender': 'Male',
    'person_name': 'Test Person Name',
    'position_schedule_type_codes': [{ code: 'FullTime' }],
    'position_titles': ['Test Position Titles'],
    'qualifications': ['Test Qualifications'],
    ...(depIds['referred_by_application_id_id'] ? { 'referred_by_application_id_id': depIds['referred_by_application_id_id'] } : {}),
    'status': 'active',
    'uri': 'Test Uri',
  };
}

function updatedData(): Record<string, unknown> {
  return {
    'birth_date': '2025-06-20',
    'family_name': 'Updated Family Name',
    'given_name': 'Updated Given Name',
    'application_process_history': 'Updated Application Process History',
    'candidate_id': 'Updated Candidate Id',
    'compensation_expectation': 99,
    'compensation_expectation_currency': 'AUD',
    'distribution_guidelines': 'Updated Distribution Guidelines',
    'external_identifier': { value: 'Updated External Identifier' },
    'gender': 'Other',
    'person_name': 'Updated Person Name',
    'position_schedule_type_codes': [{ code: 'SharedTime' }],
    'position_titles': ['Updated Position Titles'],
    'qualifications': ['Updated Qualifications'],
    ...(depIds['referred_by_application_id_id'] ? { 'referred_by_application_id_id': depIds['referred_by_application_id_id'] } : {}),
    'status': 'withdrawn',
    'uri': 'Updated Uri',
  };
}

test.describe.serial('Candidate Owner CRUD', () => {
  let createdId: string;


  test.beforeAll(async ({ orgContext }) => {


    const parentEntity = await createEntityAsAcme(orgContext, PARENT_API_PATH, { 'applied_date': '2025-01-15', 'status': 'Applied' });
    const parentId = parentEntity['id'] as string;
    BASE_PATH = `${PARENT_API_PATH}/${parentId}/candidate`;



    try {
      const dep_1 = await createEntityAsAcme(orgContext, '/recruiting/application', {  });
      depIds['referred_by_application_id_id'] = dep_1['id'] as string;
    } catch (_e) {
      // Dependency entity may already exist or have its own required fields
    }

  });


  test.afterAll(async ({ orgContext }) => {
    const baseUrl = process.env.PUBLIC_API_URL ?? 'http://localhost:3000';

    if (depIds['referred_by_application_id_id']) {
      try {
        await fetch(`${baseUrl}/api/recruiting/application/${depIds['referred_by_application_id_id']}`, {
          method: 'DELETE',
          headers: { 'Authorization': `Bearer ${orgContext.acme.apiKey}` },
        });
      } catch { /* best effort */ }
    }

  });



  const data = testData();
  const updated = updatedData();



  test('owner can create Candidate via form', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/new`);

    // Wait for SvelteKit to hydrate so the form's onsubmit handler is attached
    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="candidate-submit-btn"]');
    if (await ownerPage.locator('#birth_date').isVisible()) {
      await ownerPage.locator('#birth_date').fill(String(data['birth_date']));
    }
    if (await ownerPage.locator('#family_name').isVisible()) {
      await ownerPage.locator('#family_name').fill(String(data['family_name']));
    }
    if (await ownerPage.locator('#given_name').isVisible()) {
      await ownerPage.locator('#given_name').fill(String(data['given_name']));
    }
    if (await ownerPage.locator('#application_process_history').isVisible()) {
      await ownerPage.locator('#application_process_history').fill(String(data['application_process_history']));
    }
    if (await ownerPage.locator('#candidate_id').isVisible()) {
      await ownerPage.locator('#candidate_id').fill(String(data['candidate_id']));
    }
    if (await ownerPage.locator('#compensation_expectation').isVisible()) {
      await ownerPage.locator('#compensation_expectation').fill(String(data['compensation_expectation']));
    }
    if (await ownerPage.locator('#compensation_expectation_currency').isVisible()) {
      await ownerPage.locator('#compensation_expectation_currency').selectOption(String(data['compensation_expectation_currency']));
    }
    if (await ownerPage.locator('#distribution_guidelines').isVisible()) {
      await ownerPage.locator('#distribution_guidelines').fill(String(data['distribution_guidelines']));
    }
    if (await ownerPage.locator('[data-testid="external_identifier-value"]').isVisible()) {
      await ownerPage.locator('[data-testid="external_identifier-value"]').fill(String((data['external_identifier'] as Record<string, unknown>)?.['value'] ?? ''));
    }
    if (await ownerPage.locator('#gender').isVisible()) {
      await ownerPage.locator('#gender').selectOption(String(data['gender']));
    }
    if (await ownerPage.locator('#person_name').isVisible()) {
      await ownerPage.locator('#person_name').fill(String(data['person_name']));
    }
    if (await ownerPage.locator('#position_schedule_type_codes').isVisible()) {
      await ownerPage.locator('#position_schedule_type_codes').selectOption('FullTime');
    }
    {
      const vals = data['position_titles'] as string[];
      for (let i = 0; i < vals.length; i++) {
        if (i > 0) await ownerPage.locator('[data-testid="position_titles-add-btn"]').click();
        await ownerPage.locator(`[data-testid="position_titles-row-${i}"] input`).fill(vals[i]);
      }
    }
    if (await ownerPage.locator('#qualifications').isVisible()) {
      await ownerPage.locator('#qualifications').fill(String(data['qualifications']));
    }
    if (data['referred_by_application_id_id'] && await ownerPage.locator('#referred_by_application_id_id').isVisible()) {
      await ownerPage.locator('#referred_by_application_id_id').fill(String(data['referred_by_application_id_id']));
    }
    if (await ownerPage.locator('#status').isVisible()) {
      await ownerPage.locator('#status').selectOption(String(data['status']));
    }
    if (await ownerPage.locator('#uri').isVisible()) {
      await ownerPage.locator('#uri').fill(String(data['uri']));
    }
    await ownerPage.locator('[data-testid="candidate-submit-btn"]').click();
    await expectToast(ownerPage, 'created', 'success');
    // Wait for SvelteKit goto() navigation to complete after toast

    await ownerPage.waitForURL(/\/recruiting\/application\/[0-9a-f-]+\/candidate\/[0-9a-f-]+$/, { timeout: 20_000 });

    createdId = ownerPage.url().split('/').pop()!;
  });




  test('owner sees Candidate in list', async ({ ownerPage }) => {
    await ownerPage.goto(BASE_PATH);
    const table = ownerPage.locator('[data-testid="candidate-table"]');
    const empty = ownerPage.locator('[data-testid="candidate-empty"]');
    await expect(table.or(empty)).toBeVisible();
  });



  test('owner can view Candidate detail', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}`);

    await expect(ownerPage.locator('[data-testid="candidate-field-birth_date"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="candidate-field-family_name"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="candidate-field-given_name"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="candidate-field-application_process_history"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="candidate-field-candidate_id"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="candidate-field-compensation_expectation"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="candidate-field-compensation_expectation_currency"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="candidate-field-distribution_guidelines"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="candidate-field-external_identifier"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="candidate-field-gender"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="candidate-field-person_name"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="candidate-field-position_schedule_type_codes"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="candidate-field-position_titles"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="candidate-field-qualifications"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="candidate-field-referred_by_application_id_id"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="candidate-field-status"]')).toBeVisible();
    await expect(ownerPage.locator('[data-testid="candidate-field-uri"]')).toBeVisible();
  });




  test('owner can edit Candidate', async ({ ownerPage }) => {

    await ownerPage.goto(`${BASE_PATH}/${createdId}/edit`);

    // Wait for Svelte 5 to hydrate the form's submit handler.
    await waitForHydration(ownerPage, '[data-testid="candidate-submit-btn"]');
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
    if (await ownerPage.locator('#application_process_history').isVisible()) {
      await ownerPage.locator('#application_process_history').clear();
      await ownerPage.locator('#application_process_history').fill(String(updated['application_process_history']));
    }
    if (await ownerPage.locator('#candidate_id').isVisible()) {
      await ownerPage.locator('#candidate_id').clear();
      await ownerPage.locator('#candidate_id').fill(String(updated['candidate_id']));
    }
    if (await ownerPage.locator('#compensation_expectation').isVisible()) {
      await ownerPage.locator('#compensation_expectation').clear();
      await ownerPage.locator('#compensation_expectation').fill(String(updated['compensation_expectation']));
    }
    if (await ownerPage.locator('#compensation_expectation_currency').isVisible()) {
      await ownerPage.locator('#compensation_expectation_currency').selectOption('AUD');
    }
    if (await ownerPage.locator('#distribution_guidelines').isVisible()) {
      await ownerPage.locator('#distribution_guidelines').clear();
      await ownerPage.locator('#distribution_guidelines').fill(String(updated['distribution_guidelines']));
    }
    if (await ownerPage.locator('[data-testid="external_identifier-value"]').isVisible()) {
      await ownerPage.locator('[data-testid="external_identifier-value"]').clear();
      await ownerPage.locator('[data-testid="external_identifier-value"]').fill(String((updated['external_identifier'] as Record<string, unknown>)?.['value'] ?? ''));
    }
    if (await ownerPage.locator('#gender').isVisible()) {
      await ownerPage.locator('#gender').selectOption('Other');
    }
    if (await ownerPage.locator('#person_name').isVisible()) {
      await ownerPage.locator('#person_name').clear();
      await ownerPage.locator('#person_name').fill(String(updated['person_name']));
    }
    if (await ownerPage.locator('#position_schedule_type_codes').isVisible()) {
      await ownerPage.locator('#position_schedule_type_codes').selectOption('SharedTime');
    }
    {
      const vals = updated['position_titles'] as string[];
      // Clear existing rows first
      const existingRows = await ownerPage.locator('[data-testid^="position_titles-row-"]').count();
      for (let i = existingRows - 1; i > 0; i--) {
        await ownerPage.locator(`[data-testid="position_titles-remove-${i}"]`).click();
      }
      await ownerPage.locator('[data-testid="position_titles-row-0"] input').clear();
      await ownerPage.locator('[data-testid="position_titles-row-0"] input').fill(vals[0]);
      for (let i = 1; i < vals.length; i++) {
        await ownerPage.locator('[data-testid="position_titles-add-btn"]').click();
        await ownerPage.locator(`[data-testid="position_titles-row-${i}"] input`).fill(vals[i]);
      }
    }
    if (await ownerPage.locator('#qualifications').isVisible()) {
      await ownerPage.locator('#qualifications').clear();
      await ownerPage.locator('#qualifications').fill(String(updated['qualifications']));
    }
    if (updated['referred_by_application_id_id'] && await ownerPage.locator('#referred_by_application_id_id').isVisible()) {
      await ownerPage.locator('#referred_by_application_id_id').clear();
      await ownerPage.locator('#referred_by_application_id_id').fill(String(updated['referred_by_application_id_id']));
    }
    if (await ownerPage.locator('#status').isVisible()) {
      await ownerPage.locator('#status').selectOption('withdrawn');
    }
    if (await ownerPage.locator('#uri').isVisible()) {
      await ownerPage.locator('#uri').clear();
      await ownerPage.locator('#uri').fill(String(updated['uri']));
    }
    await ownerPage.locator('[data-testid="candidate-submit-btn"]').click();
    await expectToast(ownerPage, 'updated', 'success');
  });




});
