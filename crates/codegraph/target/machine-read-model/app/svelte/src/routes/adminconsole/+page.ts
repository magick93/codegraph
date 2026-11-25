import type { PageLoad } from './$types';

import { redirect } from '@sveltejs/kit';
import { browser } from '$app/environment';
import { currentRoles } from '$lib/roles';

const viewRoles = ['Admin'];

export const load: PageLoad = async ({ params, url }) => {
	const roles = currentRoles();
	if (browser && viewRoles.length && !roles.some((r) => viewRoles.includes(r))) {
		throw redirect(303, '/claimdetail');
	}
	const result: Record<string, any> = {};

	return result;
};
