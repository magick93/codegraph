<script lang="ts">
	import type { PageData } from './$types';
	import { transitionInfo } from './data.remote';
	import { approve } from './data.remote';
	import { escalate } from './data.remote';
	import { inspect } from './data.remote';
	import { summarize } from './data.remote';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { browser } from '$app/environment';
	import { redirect } from '@sveltejs/kit';
	import { getInfo } from './data.remote';
	import { getInfoWorkflowLive } from './data.remote';
	import Card from '$lib/components/ui/card/card.svelte';

	let { data, params }: { data: PageData; params: Record<string, string> } = $props();
	const viewParams = $derived({
		id: page.url.searchParams.get('id') ?? ''
	});
	const info_data = $derived(viewParams.id ? await getInfo(viewParams.id).catch(on_denied) : undefined);

	// Live workflow state (issue #574 S4c parity): the query.live instance
	// polls the /workflow read every 5s on the client; the fetched
	// workflow_state is the SSR fallback (kit does not start streams
	// during SSR). The badge and the transition buttons read the derived.
	const info_wf_live = browser ? getInfoWorkflowLive(viewParams['id']) : null;
	const info_workflow_state = $derived(info_wf_live?.current?.current_state ?? info_data?.workflow_state?.current_state);

	function on_denied(e: unknown): never {
		const status = (e as { status?: number })?.status;
		if (status === 401 || status === 403) {
			redirect(303, '/claimdetail');
		}
		throw e;
	}

	let actionError = $state('');

	function comp_info_edit(row: Record<string, unknown>) {
		goto(`/refundrequestform?id=${viewParams.id}`);
	}

	async function transition_info(to: string) {
		const result = await transitionInfo({ id: viewParams.id, to });
		if (result.ok) {
			void getInfo(viewParams.id).refresh();
		}
	}

	async function call_approve() {
		actionError = '';
		try {
			const result = await approve({ id: viewParams.id });
			if (!result.ok) {
				actionError = result.message ?? 'Request failed';
				return;
			}
			void getInfo(viewParams.id).refresh();

		} finally {
		}
	}

	async function call_escalate() {
		actionError = '';
		try {
			const result = await escalate({ id: viewParams.id });
			if (!result.ok) {
				actionError = result.message ?? 'Request failed';
				return;
			}
			void getInfo(viewParams.id).refresh();

		} finally {
		}
	}

	async function call_inspect() {
		actionError = '';
		try {
			const result = await inspect({ id: viewParams.id });
			if (!result.ok) {
				actionError = result.message ?? 'Request failed';
				return;
			}
			void getInfo(viewParams.id).refresh();

		} finally {
		}
	}

	async function call_summarize() {
		actionError = '';
		try {
			const result = await summarize({ id: viewParams.id });
			if (!result.ok) {
				actionError = result.message ?? 'Request failed';
				return;
			}
			void getInfo(viewParams.id).refresh();

		} finally {
		}
	}
</script>

<svelte:head>
	<title>Refund Request</title>
</svelte:head>

<div data-view="refundrequestdetail">

<h1>Refund Request</h1>
<Card
	item={info_data}
	fields={['title', 'requesterEmail', 'amount', 'urgent', 'submittedAt', 'status', 'reason', 'escalation']}
	params={params}
	testid="card"
	onedit={comp_info_edit}
/>

	<span class="workflow-state" data-testid="info-state" data-workflow-state={info_workflow_state} data-workflow-terminal={['approved', 'rejected'].includes(info_workflow_state as string) ? "true" : "false"}>{info_workflow_state}</span>
	<button type="button" data-testid="info-transition-submitted" data-transition-from="draft" data-transition-to="submitted" disabled={(info_workflow_state !== 'draft')} onclick={() => transition_info('submitted')}>Submitted</button>
	<button type="button" data-testid="info-transition-approved" data-transition-from="submitted" data-transition-to="approved" disabled={(info_workflow_state !== 'submitted')} onclick={() => transition_info('approved')}>Approved</button>
	<button type="button" data-testid="info-transition-rejected" data-transition-from="submitted" data-transition-to="rejected" disabled={(info_workflow_state !== 'submitted')} onclick={() => transition_info('rejected')}>Rejected</button>
<button type="button" data-testid="info-action-approve" onclick={() => call_approve()}>Approve</button>
<button type="button" data-testid="info-action-escalate" onclick={() => call_escalate()}>Escalate</button>
<button type="button" data-testid="info-action-inspect" onclick={() => call_inspect()}>Inspect</button>
<button type="button" data-testid="info-action-summarize" onclick={() => call_summarize()}>Summarize</button>
{#if actionError}<span class="error" data-testid="info-action-error">{actionError}</span>{/if}



</div>