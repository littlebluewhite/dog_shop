import type { PageServerLoad } from './$types';
import { serverApi } from '$lib/server/api';
import type { Address } from '$lib/types';

export const load: PageServerLoad = async (event) => {
	const addresses = await serverApi<Address[]>(event, '/api/me/addresses');
	return { addresses };
};
