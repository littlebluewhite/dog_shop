import { serverApi } from '$lib/server/api';
import type { AdminOrderListItem, Page } from '$lib/types';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async (event) => {
	const sp = event.url.searchParams;
	const qs = new URLSearchParams();
	for (const key of ['q', 'status', 'flag', 'page']) {
		const value = sp.get(key);
		if (value) qs.set(key, value);
	}
	const result = await serverApi<Page<AdminOrderListItem>>(event, `/api/admin/orders?${qs}`);
	return { result, q: sp.get('q') ?? '', status: sp.get('status') ?? '', flag: sp.get('flag') ?? '' };
};
