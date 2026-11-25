import type { PageLoad } from './$types';

import { redirect } from '@sveltejs/kit';
import { browser } from '$app/environment';
import { can } from '$lib/roles';

const viewRequires = ['ApproveRefund'];

export const load: PageLoad = async ({ params, url }) => {
	if (browser && viewRequires.length && !viewRequires.some((c) => can(c))) {
		throw redirect(303, '/claimdetail');
	}
	const result: Record<string, any> = {};

	return result;
};
