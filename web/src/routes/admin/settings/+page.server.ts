import type { PageServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { AllSettings } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	const settings = await serverApi<AllSettings>(event, '/api/admin/settings');
	return { settings };
};
