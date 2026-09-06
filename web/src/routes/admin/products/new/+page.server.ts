import type { PageServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { Category } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	const categories = await serverApi<Category[]>(event, '/api/admin/categories');
	return { categories };
};
