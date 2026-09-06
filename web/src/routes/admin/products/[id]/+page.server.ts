import { error } from '@sveltejs/kit';
import type { PageServerLoad } from './$types';
import { ApiError, serverApi } from '$lib/server/api';
import type { AdminProduct, Category } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	try {
		const [product, categories] = await Promise.all([
			serverApi<AdminProduct>(event, `/api/admin/products/${event.params.id}`),
			serverApi<Category[]>(event, '/api/admin/categories')
		]);
		return { product, categories };
	} catch (e) {
		// 404 = 沒這個商品；400 = id 不是 uuid
		if (e instanceof ApiError && (e.status === 404 || e.status === 400)) error(404, '找不到這個商品');
		throw e;
	}
};
