<!-- Gate-owned stub (entity ui gate). -->
<script lang="ts">
	import type { Snippet } from 'svelte';
	let { children, value = $bindable(''), ...rest }: {
		children?: Snippet;
		value?: string;
		[key: string]: unknown;
	} = $props();
</script>

{@render children?.()}
