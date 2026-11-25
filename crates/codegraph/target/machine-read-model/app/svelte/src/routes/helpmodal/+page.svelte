<script lang="ts">
	import type { PageData } from './$types';
	import Dialog from '$lib/components/ui/dialog/dialog.svelte';
	import Card from '$lib/components/ui/card/card.svelte';

	let { data, params }: { data: PageData; params: Record<string, string> } = $props();
	let dialog_open = $state(true);

	function close_dialog() {
		dialog_open = false;
		history.back();
	}

</script>

<svelte:head>
	<title>Help</title>
</svelte:head>

<div data-view="helpmodal">
<Dialog bind:open={dialog_open} testid="dialog">

<h1>Help</h1>
<Card
	item={undefined}
	fields={['title']}
	params={params}
	testid="card"
/>




	<button type="button" data-testid="helpmodal-modal-close" onclick={close_dialog}>Close</button>
</Dialog>

</div>