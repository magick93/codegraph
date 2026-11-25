<script lang="ts">
	import type { PageData } from './$types';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { redirect } from '@sveltejs/kit';
	import { getGrid } from './data.remote';
	import Button from '$lib/components/ui/button/button.svelte';

	let { data, params }: { data: PageData; params: Record<string, string> } = $props();

	const grid_data = $derived(await getGrid({ page: page.url.searchParams.get('page') ?? '0', pageSize: page.url.searchParams.get('page_size') ?? '20' }).catch(on_denied));


	function on_denied(e: unknown): never {
		const status = (e as { status?: number })?.status;
		if (status === 401 || status === 403) {
			redirect(303, '/claimdetail');
		}
		throw e;
	}


	function comp_grid_review(row: Record<string, unknown>) {
		goto(`/refundrequestdetail?id=${row.id}`);
	}

	function comp_grid_select(row: Record<string, unknown>) {
		goto(`/refundrequestdetail?id=${row.id}`);
	}
</script>

<svelte:head>
	<title>Refund Requests</title>
</svelte:head>

<div data-view="refundrequestlist">

<h1>Refund Requests</h1>
<table data-testid="grid-table">
	<thead>
		<tr>
		
			<th>title</th>
		
			<th>status</th>
		
			<th>amount</th>
		
			<th>urgent</th>
		
			<th>submittedAt</th>
		
		</tr>
	</thead>
	<tbody>
		{#each grid_data as item}
			<tr data-testid="grid-row" onclick={() => comp_grid_review(item)}>
			
				<td>{item.title}</td>
			
				<td>{item.status}</td>
			
				<td>{item.amount}</td>
			
				<td>{item.urgent}</td>
			
				<td>{item.submittedAt}</td>
			
			<td><span class="workflow-state" data-testid="grid-state" data-workflow-state={item.status} data-workflow-terminal={['approved', 'rejected'].includes(item.status as string) ? "true" : "false"}>{item.status}</span></td>
			</tr>
		{/each}
	</tbody>
</table>




</div>