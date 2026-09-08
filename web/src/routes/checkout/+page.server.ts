import type { PageServerLoad } from './$types';
import { ApiError, serverApi } from '$lib/server/api';
import type { Address, CvsStore } from '$lib/types';

/** 會員帶常用地址；?store=<token> 是計畫 4 地圖選完門市回來的入口，本計畫先能還原顯示 */
export const load: PageServerLoad = async (event) => {
	const addresses = event.locals.user ? await serverApi<Address[]>(event, '/api/me/addresses') : [];
	const token = event.url.searchParams.get('store');
	let store: CvsStore | null = null;
	if (token) {
		try {
			store = await serverApi<CvsStore>(event, `/api/checkout/cvs-store/${encodeURIComponent(token)}`);
		} catch (e) {
			if (!(e instanceof ApiError && e.status === 404)) throw e;
		}
	}
	return { addresses, store, storeError: event.url.searchParams.get('store_error'), title: '結帳' };
};
