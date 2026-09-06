import type { PageServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { Category, Page, ProductListItem } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	const sp = event.url.searchParams;
	const qs = new URLSearchParams();
	for (const key of ['q', 'category', 'sort', 'page']) {
		const value = sp.get(key);
		if (value) qs.set(key, value);
	}
	const [result, categories] = await Promise.all([
		serverApi<Page<ProductListItem>>(event, `/api/products?${qs}`),
		serverApi<Category[]>(event, '/api/categories')
	]);
	return {
		result,
		categories,
		q: sp.get('q') ?? '',
		category: sp.get('category') ?? '',
		sort: sp.get('sort') ?? 'newest'
	};
};
