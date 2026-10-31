// Server-side load for the hand-authored kanban board.
//
// NOT generated — parked at `(app)/kanban/`, outside any modeled domain's
// route segment, so regeneration never sweeps it (the sweep removes stale
// dirs under `(app)/{domain}/{path_segment}/`; the domain here is `board`).
// See docs/lexorank-kanban.md §3 gap 6.
//
// Fetches run server-side through $lib/server/api (issue #574 contract —
// the API origin and its credentials never reach the browser). Cards are
// read in rank order via the generated ux sort plane (?sort=rank&order=asc,
// validated against the entity's allow-list, tie-broken by id ASC).
import { error } from '@sveltejs/kit';
import { apiFetch } from '$lib/server/api';
import type { PageServerLoad } from './$types';

type Json = Record<string, any>;

async function list(path: string, apiKey: string): Promise<Json[]> {
	const res = await apiFetch(path, {
		headers: { Authorization: `Bearer ${apiKey}` }
	});
	if (!res.ok) throw error(res.status, `GET ${path}: ${res.statusText}`);
	const body = (await res.json()) as { data: Json[] };
	return body.data ?? [];
}

export const load: PageServerLoad = async ({ locals }) => {
	const apiKey = locals.apiKey;
	if (!apiKey) throw error(401, 'Not authenticated');

	// Single-board assumption for the sketch — pick the first board.
	const boards = await list('/api/v1/board/boards', apiKey);
	const board = boards[0];
	if (!board) return { board: null, columns: [] };

	const lists = await list(
		`/api/v1/board/lists?filter[board_id]=${board.id}&sort=position&order=asc&page_size=100`,
		apiKey
	);
	const columns = await Promise.all(
		lists.map(async (l) => ({
			list: l,
			cards: await list(
				`/api/v1/board/lists/${l.id}/cards?sort=rank&order=asc&page_size=500`,
				apiKey
			)
		}))
	);
	return { board, columns };
};
