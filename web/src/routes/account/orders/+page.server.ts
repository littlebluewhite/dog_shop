import type { PageServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { OrderSummary, Page } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	const page = Number(event.url.searchParams.get('page') ?? '1') || 1;
	const result = await serverApi<Page<OrderSummary>>(event, `/api/me/orders?page=${page}`);
	return { result };
};
