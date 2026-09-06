import type { LayoutServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { ShopSettings } from '$lib/types';

export const load: LayoutServerLoad = async (event) => {
	const shop = await serverApi<ShopSettings>(event, '/api/settings/public');
	return { user: event.locals.user, shop };
};
