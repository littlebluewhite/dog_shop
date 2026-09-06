import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { ApiError, serverApi } from '$lib/server/api';
import type { ProductDetail } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	try {
		const product = await serverApi<ProductDetail>(event, `/api/products/${encodeURIComponent(event.params.slug)}`);
		return { product };
	} catch (e) {
		if (e instanceof ApiError && e.status === 404) error(404, '找不到這個商品');
		throw e;
	}
};
