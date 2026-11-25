<script lang="ts">
	import type { PageData } from './$types';
	import { goto } from '$app/navigation';
	import Tabs from '$lib/components/ui/tabs/tabs.svelte';
	import Card from '$lib/components/ui/card/card.svelte';

	let { data, params }: { data: PageData; params: Record<string, string> } = $props();

	function comp_portal_browse(row: Record<string, unknown>) {
		goto("/refundrequestlist");
	}

	function comp_portal_help(row: Record<string, unknown>) {
		goto("/helpmodal?dialog=open");
	}
</script>

<svelte:head>
	<title>Refund Portal</title>
</svelte:head>

<div data-view="home">
<Tabs testid="tabs">

<h1>Refund Portal</h1>
<Card
	item={undefined}
	fields={['title']}
	params={params}
	testid="card"
	onbrowse={comp_portal_browse}
	onhelp={comp_portal_help}
/>



<h2 class="container-label" data-testid="shipping-label">Shipping</h2>
<h2 class="container-label" data-testid="payment-label">Payment</h2>

</Tabs>

</div>