<script lang="ts">
	import type { PageData } from './$types';
	import { page } from '$app/state';
	import { redirect } from '@sveltejs/kit';
	import { getQueue } from './data.remote';
	import Button from '$lib/components/ui/button/button.svelte';

	let { data, params }: { data: PageData; params: Record<string, string> } = $props();

	const queue_data = $derived(await getQueue({ page: page.url.searchParams.get('page') ?? '0', pageSize: page.url.searchParams.get('page_size') ?? '20' }).catch(on_denied));


	function on_denied(e: unknown): never {
		const status = (e as { status?: number })?.status;
		if (status === 401 || status === 403) {
			redirect(303, '/claimdetail');
		}
		throw e;
	}

</script>

<svelte:head>
	<title>Review Queue</title>
</svelte:head>

<div data-view="reviewqueue">

<h1>Review Queue</h1>
<table data-testid="queue-table">
	<thead>
		<tr>
		
			<th>title</th>
		
			<th>status</th>
		
		</tr>
	</thead>
	<tbody>
		{#each queue_data as item}
			<tr data-testid="queue-row">
			
				<td>{item.title}</td>
			
				<td>{item.status}</td>
			
			<td><span class="workflow-state" data-testid="queue-state" data-workflow-state={item.status} data-workflow-terminal={['approved', 'rejected'].includes(item.status as string) ? "true" : "false"}>{item.status}</span></td>
			</tr>
		{/each}
	</tbody>
</table>




</div>