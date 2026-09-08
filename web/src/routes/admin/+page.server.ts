import { serverApi } from '$lib/server/api';
import type { Dashboard } from '$lib/types';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async (event) => {
	const dashboard = await serverApi<Dashboard>(event, '/api/admin/dashboard');
	return { dashboard };
};
