import { error } from '@sveltejs/kit';
import { ApiError, serverApi } from '$lib/server/api';
import type { AdminOrderDetail } from '$lib/types';
import type { PageServerLoad } from './$types';

export const load: PageServerLoad = async (event) => {
	try {
		const order = await serverApi<AdminOrderDetail>(event, `/api/admin/orders/${encodeURIComponent(event.params.id)}`);
		return { order };
	} catch (e) {
		if (e instanceof ApiError && (e.status === 404 || e.status === 400)) error(404, '找不到這筆訂單');
		throw e;
	}
};
