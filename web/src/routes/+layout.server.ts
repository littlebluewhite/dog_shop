import type { LayoutServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { PublicSettings } from '$lib/types';

export const load: LayoutServerLoad = async (event) => {
	const settings = await serverApi<PublicSettings>(event, '/api/settings/public');
	return { user: event.locals.user, shop: settings.shop, settings };
};
