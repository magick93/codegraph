<script lang="ts">
	import type { PageData } from './$types';
	import { transitionGuardedInfo } from './data.remote';
	import { approve } from './data.remote';
	import { escalate } from './data.remote';
	import { page } from '$app/state';
	import { browser } from '$app/environment';
	import { redirect } from '@sveltejs/kit';
	import { getGuardedInfo } from './data.remote';
	import { getGuardedInfoWorkflowLive } from './data.remote';
	import Card from '$lib/components/ui/card/card.svelte';

	let { data, params }: { data: PageData; params: Record<string, string> } = $props();
	const viewParams = $derived({
		id: page.url.searchParams.get('id') ?? ''
	});
	const guarded_info_data = $derived(viewParams.id ? await getGuardedInfo(viewParams.id).catch(on_denied) : undefined);

	// Live workflow state (issue #574 S4c parity): the query.live instance
	// polls the /workflow read every 5s on the client; the fetched
	// workflow_state is the SSR fallback (kit does not start streams
	// during SSR). The badge and the transition buttons read the derived.
	const guarded_info_wf_live = browser ? getGuardedInfoWorkflowLive(viewParams['id']) : null;
	const guarded_info_workflow_state = $derived(guarded_info_wf_live?.current?.current_state ?? guarded_info_data?.workflow_state?.current_state);

	function on_denied(e: unknown): never {
		const status = (e as { status?: number })?.status;
		if (status === 401 || status === 403) {
			redirect(303, '/claimdetail');
		}
		throw e;
	}

	let actionError = $state('');

	async function transition_guarded_info(to: string) {
		const result = await transitionGuardedInfo({ id: viewParams.id, to });
		if (result.ok) {
			void getGuardedInfo(viewParams.id).refresh();
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
			void getGuardedInfo(viewParams.id).refresh();

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
			void getGuardedInfo(viewParams.id).refresh();

		} finally {
		}
	}
</script>

<svelte:head>
	<title>Guarded Refund Request</title>
</svelte:head>

<div data-view="guardedrefunddetail">

<h1>Guarded Refund Request</h1>
<Card
	item={guarded_info_data}
	fields={['title', 'requesterEmail', 'amount', 'urgent', 'submittedAt', 'status', 'reason', 'escalation']}
	params={params}
	testid="card"
/>

	<span class="workflow-state" data-testid="guarded_info-state" data-workflow-state={guarded_info_workflow_state} data-workflow-terminal={['approved', 'rejected'].includes(guarded_info_workflow_state as string) ? "true" : "false"}>{guarded_info_workflow_state}</span>
	<button type="button" data-testid="guarded_info-transition-submitted" data-transition-from="draft" data-transition-to="submitted" disabled={(guarded_info_workflow_state !== 'draft')} onclick={() => transition_guarded_info('submitted')}>Submitted</button>
	<button type="button" data-testid="guarded_info-transition-approved" data-transition-from="submitted" data-transition-to="approved" disabled={(guarded_info_workflow_state !== 'submitted')} onclick={() => transition_guarded_info('approved')}>Approved</button>
	<button type="button" data-testid="guarded_info-transition-rejected" data-transition-from="submitted" data-transition-to="rejected" disabled={(guarded_info_workflow_state !== 'submitted')} onclick={() => transition_guarded_info('rejected')}>Rejected</button>
<button type="button" data-testid="guarded_info-action-approve" onclick={() => call_approve()}>Approve</button>
<button type="button" data-testid="guarded_info-action-escalate" onclick={() => call_escalate()}>Escalate</button>
{#if actionError}<span class="error" data-testid="guarded_info-action-error">{actionError}</span>{/if}



</div>