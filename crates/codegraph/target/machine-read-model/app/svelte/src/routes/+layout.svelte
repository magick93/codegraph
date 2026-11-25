<script lang="ts">
	import NavigationMenu from '$lib/components/ui/navigation-menu/navigation-menu.svelte';

	let { children }: { children: import('svelte').Snippet } = $props();
</script>

<NavigationMenu testid="navigation-menu">
	<a href={"/refundrequestlist"}>Refund Requests</a>
	<a href={"/helpmodal"}>Help</a>
</NavigationMenu>

{@render children()}
