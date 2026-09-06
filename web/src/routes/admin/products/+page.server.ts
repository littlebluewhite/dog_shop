import type { PageServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { AdminProductListItem, Page } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	const sp = event.url.searchParams;
	const qs = new URLSearchParams();
	for (const key of ['q', 'status', 'page']) {
		const value = sp.get(key);
		if (value) qs.set(key, value);
	}
	const result = await serverApi<Page<AdminProductListItem>>(event, `/api/admin/products?${qs}`);
	return { result, q: sp.get('q') ?? '', status: sp.get('status') ?? '' };
};
