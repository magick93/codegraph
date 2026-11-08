// Same-origin proxy for the hand-written LexoRank endpoints.
//
// Sits at /kanban/api (a CHILD route of the board page — +page.server.ts and
// +server.ts cannot share a route, so the POST plane lives one segment down).
// The browser talks only to this same-origin endpoint; apiFetch forwards the
// session credentials server-side (issue #574 contract).
//
// Not generated — survives regeneration like the rest of the kanban route.
import { error } from '@sveltejs/kit';
import { apiFetch } from '$lib/server/api';
import type { RequestHandler } from './$types';

type ProxyBody = {
	op: 'move' | 'rebalance';
	card_id?: string;
	list_id?: string;
	payload?: Record<string, unknown>;
};

export const POST: RequestHandler = async ({ request, locals }) => {
	if (!locals.apiKey) throw error(401, 'Not authenticated');

	let body: ProxyBody;
	try {
		body = (await request.json()) as ProxyBody;
	} catch {
		throw error(400, 'invalid JSON body');
	}

	const path =
		body.op === 'move'
			? `/api/v1/board/cards/${body.card_id}/move`
			: `/api/v1/board/lists/${body.list_id}/rebalance`;
	if (!body.card_id && !body.list_id) throw error(400, 'card_id or list_id required');

	const res = await apiFetch(path, {
		method: 'POST',
		headers: {
			Authorization: `Bearer ${locals.apiKey}`,
			'Content-Type': 'application/json'
		},
		body: JSON.stringify(body.payload ?? {})
	});
	const text = await res.text();
	return new Response(text, {
		status: res.status,
		headers: { 'content-type': 'application/json' }
	});
};
