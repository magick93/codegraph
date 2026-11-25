<script lang="ts">
	import type { PageData } from './$types';
	import { submitEditor } from './data.remote';
	import { transitionEditor } from './data.remote';
	import { z } from 'zod';
	import { createAppForm } from '$lib/forms/form';
	import SelectVariant from '$lib/forms/fields/select-variant.svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { browser } from '$app/environment';
	import { redirect } from '@sveltejs/kit';
	import { getEditor } from './data.remote';
	import { getEditorWorkflowLive } from './data.remote';
	import TextField from '$lib/forms/fields/text.svelte';
	import NumberField from '$lib/forms/fields/number.svelte';
	import EmailField from '$lib/forms/fields/email.svelte';
	import DatetimeField from '$lib/forms/fields/datetime.svelte';
	import CheckboxField from '$lib/forms/fields/checkbox.svelte';
	import SelectField from '$lib/forms/fields/select.svelte';
	import Button from '$lib/components/ui/button/button.svelte';
	import { uploadReceipt } from './data.remote';

	let { data, params }: { data: PageData; params: Record<string, string> } = $props();
	const viewParams = $derived({
		id: page.url.searchParams.get('id') ?? ''
	});
	const editor_data = $derived(viewParams.id ? await getEditor(viewParams.id).catch(on_denied) : undefined);

	// Live workflow state (issue #574 S4c parity): the query.live instance
	// polls the /workflow read every 5s on the client; the fetched
	// workflow_state is the SSR fallback (kit does not start streams
	// during SSR). The badge and the transition buttons read the derived.
	const editor_wf_live = browser ? getEditorWorkflowLive(viewParams['id']) : null;
	const editor_workflow_state = $derived(editor_wf_live?.current?.current_state ?? editor_data?.workflow_state?.current_state);

	function on_denied(e: unknown): never {
		const status = (e as { status?: number })?.status;
		if (status === 401 || status === 403) {
			redirect(303, '/claimdetail');
		}
		throw e;
	}

	const editor_form_state = $derived((editor_data ?? {}) as Record<string, any>);
	let submitting = $state(false);
	let formError = $state('');
	let fieldErrors = $state<Record<string, string>>({});

	function comp_editor_cancel(row: Record<string, unknown>) {
		goto("/refundrequestlist");
	}

	function comp_editor_save(row: Record<string, unknown>) {
		goto(`/refundrequestdetail?id=${viewParams.id}`);
	}
	// Media file field 'receipt' (issue #633): the upload posts
	// multipart FormData through the colocated data.remote.ts command —
	// the submit payload never carries the File object.
	let media_url_receipt = $state<string | null>(null);
	let media_busy_receipt = $state(false);
	let media_error_receipt = $state<string | null>(null);

	async function handleFileReceipt(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		input.value = '';
		if (!file) return;
		media_busy_receipt = true;
		media_error_receipt = null;
		try {
			const result = await uploadReceipt({ id: String(viewParams.id), file });
			if (!result.ok) {
				media_error_receipt = result.message ?? 'Upload failed';
				return;
			}
			media_url_receipt = result.url ?? null;
		} finally {
			media_busy_receipt = false;
		}
	}


const editorSchema = z.object({
	title: z.string().min(2, "Title too short"),
	amount: z.number().optional(),
	requesterEmail: z.email().optional(),
	submittedAt: z.string().optional(),
	urgent: z.boolean().optional(),
	reason: z.enum(["Damaged", "WrongItem", "LateDelivery", "Other"]).optional(),
});

	const editorForm = createAppForm(() => ({
		defaultValues: {
			title: (editor_form_state.title ?? '') as string,
			amount: (editor_form_state.amount ?? undefined) as number | undefined,
			requesterEmail: ((editor_form_state.requesterEmail || undefined)) as string | undefined,
			submittedAt: ((editor_form_state.submittedAt || undefined)) as string | undefined,
			urgent: (editor_form_state.urgent ?? undefined) as boolean | undefined,
			reason: ((editor_form_state.reason || undefined)) as string | undefined,
		},
		validators: { onSubmit: editorSchema as never },
		onSubmit: async ({ value }) => {
			formError = '';
			fieldErrors = {};
			const payload: Record<string, unknown> = {};
			payload.title = value.title;
			payload.amount = value.amount;
			payload.requester_email = value.requesterEmail;
			payload.submitted_at = value.submittedAt === '' || value.submittedAt === undefined ? null : new Date(String(value.submittedAt)).toISOString();
			payload.urgent = value.urgent === true;
			payload.reason = value.reason;
			submitting = true;
			try {
				const result = await submitEditor({
					id: viewParams.id || null,
					data: payload
				});
				if (!result.ok) {
					formError = result.message ?? 'Request failed';
					fieldErrors = result.fieldErrors ?? {};
					return;
				}
				goto(`/refundrequestdetail?id=${result.id ?? viewParams.id}`);
			} finally {
				submitting = false;
			}
		}
	}));

	async function transition_editor(to: string) {
		const result = await transitionEditor({ id: viewParams.id, to });
		if (result.ok) {
			void getEditor(viewParams.id).refresh();
		}
	}
</script>

<svelte:head>
	<title>Edit Refund Request</title>
</svelte:head>

<div data-view="refundrequestform">

<form data-testid="editor-form" onsubmit={(e) => { e.preventDefault(); editorForm.handleSubmit(); }}>
	
	<editorForm.AppField name="title">
		{#snippet children(field)}
		<TextField name="title" testid="editor-title" required message="Title too short" serverMessage={fieldErrors.title} />
		{/snippet}
	</editorForm.AppField>
	
	<editorForm.AppField name="amount">
		{#snippet children(field)}
		<NumberField name="amount" testid="editor-amount" serverMessage={fieldErrors.amount} />
		{/snippet}
	</editorForm.AppField>
	
	<editorForm.AppField name="requesterEmail">
		{#snippet children(field)}
		<EmailField name="requesterEmail" testid="editor-requesterEmail" serverMessage={fieldErrors.requesterEmail} />
		{/snippet}
	</editorForm.AppField>
	
	<editorForm.AppField name="submittedAt">
		{#snippet children(field)}
		<DatetimeField name="submittedAt" testid="editor-submittedAt" serverMessage={fieldErrors.submittedAt} />
		{/snippet}
	</editorForm.AppField>
	
	<editorForm.AppField name="urgent">
		{#snippet children(field)}
		<CheckboxField name="urgent" testid="editor-urgent" serverMessage={fieldErrors.urgent} />
		{/snippet}
	</editorForm.AppField>
	
	<editorForm.AppField name="reason">
		{#snippet children(field)}
		<SelectVariant name="reason" data-testid="editor-reason" options={["Damaged", "WrongItem", "LateDelivery", "Other"]} onchange={(e) => field.handleChange(e.currentTarget.value)} value={field.state.value} />
		{/snippet}
	</editorForm.AppField>
	
<label>
				receipt
				<input type="file" data-testid="media-receipt-input" accept="image/png,image/jpeg,application/pdf" onchange={handleFileReceipt} />
				<img data-testid="media-receipt-display" src={media_url_receipt ?? ''} alt="receipt" hidden={!media_url_receipt} />
				<span data-testid="media-receipt-busy" hidden={!media_busy_receipt}>Uploading…</span>
				<span data-testid="media-receipt-error" role="alert" hidden={!media_error_receipt}>{media_error_receipt ?? ''}</span>
			</label>


	{#if formError}<span class="error" data-testid="editor-error">{formError}</span>{/if}

	
	<editorForm.Subscribe selector={({ canSubmit, isSubmitting }) => ({ canSubmit, isSubmitting })}>
		{#snippet children(sub)}
		<button type="submit" data-testid="button" disabled={!sub.canSubmit || sub.isSubmitting}>Submit</button>
		{/snippet}
	</editorForm.Subscribe>
	
	<span class="workflow-state" data-testid="editor-state" data-workflow-state={editor_workflow_state} data-workflow-terminal={['approved', 'rejected'].includes(editor_workflow_state as string) ? "true" : "false"}>{editor_workflow_state}</span>
	<button type="button" data-testid="editor-transition-submitted" data-transition-from="draft" data-transition-to="submitted" disabled={(editor_workflow_state !== 'draft')} onclick={() => transition_editor('submitted')}>Submitted</button>
	<button type="button" data-testid="editor-transition-approved" data-transition-from="submitted" data-transition-to="approved" disabled={(editor_workflow_state !== 'submitted')} onclick={() => transition_editor('approved')}>Approved</button>
	<button type="button" data-testid="editor-transition-rejected" data-transition-from="submitted" data-transition-to="rejected" disabled={(editor_workflow_state !== 'submitted')} onclick={() => transition_editor('rejected')}>Rejected</button>

</form>
<Button onclick={comp_editor_cancel} testid="editor-cancel">Cancel</Button>




</div>