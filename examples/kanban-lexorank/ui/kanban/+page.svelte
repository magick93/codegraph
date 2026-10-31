<script lang="ts">
	// Hand-authored kanban board — the drag-and-drop surface codegraph's
	// generated UI does not have (docs/lexorank-kanban.md §3 gap 6).
	//
	// Uses the canonical svelte-dnd-action multi-zone recipe: one dnd zone
	// per column over a flat item array tagged with `listId`; on finalize we
	// diff the zone's new order, derive {after_id?, before_id?, list_id?},
	// and let the SERVER compute the midpoint key (server-authoritative V2).
	// The dependency comes from the ui/scaffold/package_json.tera override.
	import { dndzone, type DndEventInfo } from 'svelte-dnd-action';

	type Card = { id: string; title: string; rank: string | null; list_id: string };
	type ListMeta = { id: string; name: string; position: number };
	type Column = { list: ListMeta; cards: Card[] };
	type Item = Card & { listId: string };

	let { data } = $props();

	let board = $state(data.board);
	let columns: Column[] = data.columns;
	// Flat, authoritative item store — every zone renders a filtered view.
	let items = $state<Item[]>(
		columns.flatMap((c) => c.cards.map((card) => ({ ...card, listId: c.list.id })))
	);
	let saving = $state(false);
	let notice = $state('');

	function itemsFor(listId: string): Item[] {
		return items.filter((i) => i.listId === listId);
	}

	// The zone identity we pass via dndzone({ id }) comes back in
	// info.id — we use the list's id.
	async function handleFinalize(e: CustomEvent<DndEventInfo<Item>>) {
		const listId = String(e.detail.info.id);
		const newOrder = e.detail.items as Item[];

		// The moved card is the one present in the zone's new order but whose
		// previous position differs — compare against the old order of THIS zone.
		const before = itemsFor(listId).map((i) => i.id);
		const after = newOrder.map((i) => i.id);
		const changed = after.filter((id, idx) => before[idx] !== id);
		const movedId = changed[0];
		if (!movedId) {
			items = applyZoneOrder(items, listId, newOrder);
			return;
		}

		const index = after.indexOf(movedId);
		const payload = {
			after_id: index > 0 ? after[index - 1] : undefined,
			before_id: index < after.length - 1 ? after[index + 1] : undefined,
			list_id: movedId && items.find((i) => i.id === movedId)?.listId !== listId
				? listId
				: undefined
		};

		// Optimistic local update.
		items = applyZoneOrder(items, listId, newOrder);

		saving = true;
		notice = '';
		try {
			const res = await fetch('/kanban/api', {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({ op: 'move', card_id: movedId, payload })
			});
			if (!res.ok) {
				// 409 rank_exhausted → surface the rebalance hint (§6 of the recipe).
				const body = (await res.json().catch(() => null)) as { error?: { message?: string } };
				notice = body?.error?.message ?? `move failed (${res.status})`;
			}
		} catch (err) {
			notice = String(err);
		} finally {
			saving = false;
		}
	}

	/** Replace one zone's slice of the flat store with the finalized order. */
	function applyZoneOrder(all: Item[], listId: string, zoneOrder: Item[]): Item[] {
		const rest = all.filter((i) => i.listId !== listId);
		const zone = zoneOrder.map((i) => ({ ...i, listId }));
		return [...rest, ...zone];
	}

	async function rebalance(listId: string) {
		saving = true;
		notice = '';
		try {
			const res = await fetch('/kanban/api', {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({ op: 'rebalance', list_id: listId })
			});
			if (!res.ok) notice = `rebalance failed (${res.status})`;
		} finally {
			saving = false;
		}
	}

	const dndOptions = (listId: string) => ({
		items: itemsFor(listId),
		id: listId,
		flipDurationMs: 150,
		dragDisabled: saving,
		dropTargetStyle: { outline: '2px dashed #64748b', outlineOffset: '-2px' }
	});
</script>

<svelte:head><title>Board</title></svelte:head>

<h1>{board?.name ?? 'No board yet'}</h1>
{#if notice}
	<p role="alert" style="color: #b91c1c">{notice}</p>
{/if}

<div class="board" style="display: flex; gap: 1.5rem; align-items: flex-start;">
	{#each columns as column (column.list.id)}
		<section class="column" style="min-width: 18rem;">
			<header style="display: flex; justify-content: space-between; align-items: baseline;">
				<h2 style="font-size: 1rem;">{column.list.name}</h2>
				<button onclick={() => rebalance(column.list.id)} disabled={saving}>rebalance</button>
			</header>

			<ul
				use:dndzone={dndOptions(column.list.id)}
				on:finalize={handleFinalize}
				style="list-style: none; margin: 0; padding: 0.5rem; min-height: 6rem; background: #f1f5f9; border-radius: 0.5rem;"
			>
				{#each itemsFor(column.list.id) as card (card.id)}
					<li
						style="background: white; border: 1px solid #e2e8f0; border-radius: 0.375rem; padding: 0.5rem 0.75rem; margin-bottom: 0.5rem; cursor: grab;"
						title={card.rank ? `rank ${card.rank}` : 'unranked'}
					>
						{card.title}
					</li>
				{/each}
			</ul>
		</section>
	{/each}
</div>
