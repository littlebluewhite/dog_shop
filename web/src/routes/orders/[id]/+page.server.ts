import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { ApiError, serverApi } from '$lib/server/api';
import type { OrderDetail } from '$lib/types';

/** 會員看自己的不用 token；訪客帶 ?t=。看不到（或 id 不是 uuid）一律 404 */
export const load: PageServerLoad = async (event) => {
	const token = event.url.searchParams.get('t');
	const qs = token ? `?t=${encodeURIComponent(token)}` : '';
	try {
		const order = await serverApi<OrderDetail>(event, `/api/orders/${encodeURIComponent(event.params.id)}${qs}`);
		return { order, token };
	} catch (e) {
		if (e instanceof ApiError && (e.status === 404 || e.status === 400)) error(404, '找不到這筆訂單');
		throw e;
	}
};
