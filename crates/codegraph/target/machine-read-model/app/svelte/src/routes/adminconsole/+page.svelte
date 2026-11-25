<script lang="ts">
	import type { PageData } from './$types';
	import Card from '$lib/components/ui/card/card.svelte';

	let { data, params }: { data: PageData; params: Record<string, string> } = $props();
</script>

<svelte:head>
	<title>Admin Console</title>
</svelte:head>

<div data-view="adminconsole">

<h1>Admin Console</h1>
<Card
	item={undefined}
	fields={['title']}
	params={params}
	testid="card"
/>




</div>