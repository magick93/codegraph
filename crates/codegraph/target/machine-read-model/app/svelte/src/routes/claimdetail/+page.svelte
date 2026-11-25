<script lang="ts">
	import type { PageData } from './$types';
	import { transitionClaimInfo } from './data.remote';
	import { page } from '$app/state';
	import { browser } from '$app/environment';
	import { redirect } from '@sveltejs/kit';
	import { getClaimInfo } from './data.remote';
	import { getClaimInfoWorkflowLive } from './data.remote';
	import Card from '$lib/components/ui/card/card.svelte';

	let { data, params }: { data: PageData; params: Record<string, string> } = $props();
	const viewParams = $derived({
		id: page.url.searchParams.get('id') ?? ''
	});
	const claim_info_data = $derived(viewParams.id ? await getClaimInfo(viewParams.id).catch(on_denied) : undefined);

	// Live workflow state (issue #574 S4c parity): the query.live instance
	// polls the /workflow read every 5s on the client; the fetched
	// workflow_state is the SSR fallback (kit does not start streams
	// during SSR). The badge and the transition buttons read the derived.
	const claim_info_wf_live = browser ? getClaimInfoWorkflowLive(viewParams['id']) : null;
	const claim_info_workflow_state = $derived(claim_info_wf_live?.current?.current_state ?? claim_info_data?.workflow_state?.current_state);

	function on_denied(e: unknown): never {
		const status = (e as { status?: number })?.status;
		if (status === 401 || status === 403) {
			redirect(303, '/claimdetail');
		}
		throw e;
	}


	// The machine read model (issue #817): transition buttons render from
	// the DERIVED edge set (`available_transitions`) — the UI never runs a
	// machine, so it can never offer a transition the backend would 409.
	// The key/label helpers mirror the generation-time naming contract the
	// page classes use.
	function machineTransitionKey(to: string): string {
		return to.replace(/([a-z0-9])([A-Z])/g, '$1-$2').replace(/[\s_]+/g, '-').toLowerCase();
	}
	function machineTransitionLabel(to: string): string {
		const spaced = to.replace(/[_-]/g, ' ');
		return spaced.charAt(0).toUpperCase() + spaced.slice(1);
	}

	async function transition_claim_info(to: string) {
		const result = await transitionClaimInfo({ id: viewParams.id, to });
		if (result.ok) {
			void getClaimInfo(viewParams.id).refresh();
		}
	}
</script>

<svelte:head>
	<title>Claim</title>
</svelte:head>

<div data-view="claimdetail">

<h1>Claim</h1>
<Card
	item={claim_info_data}
	fields={['label', 'status']}
	params={params}
	testid="card"
/>

	<span class="workflow-state" data-testid="claim_info-state" data-workflow-state={claim_info_workflow_state} data-workflow-terminal={['submitted'].includes(claim_info_workflow_state as string) ? "true" : "false"}>{claim_info_workflow_state}</span>
	{#each claim_info_wf_live?.current?.machine?.available_transitions ?? claim_info_data?.workflow_state?.machine?.available_transitions ?? [] as t}
	<button type="button" data-testid="claim_info-transition-{machineTransitionKey(t.to)}" data-transition-event={t.event} data-transition-to={t.to} onclick={() => transition_claim_info(t.to)}>{machineTransitionLabel(t.to)}</button>
	{/each}



</div>