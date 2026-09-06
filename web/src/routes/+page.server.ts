import type { PageServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { Category, Page, ProductListItem } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	const [latest, categories] = await Promise.all([
		serverApi<Page<ProductListItem>>(event, '/api/products?sort=newest&per_page=8'),
		serverApi<Category[]>(event, '/api/categories')
	]);
	return { latest: latest.items, categories };
};
