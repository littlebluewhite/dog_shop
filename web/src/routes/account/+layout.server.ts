import { redirect } from '@sveltejs/kit';
import type { LayoutServerLoad } from './$types';

/** /account 底下要登入；沒登入就去登入頁（規格 §6.2） */
export const load: LayoutServerLoad = ({ locals, url }) => {
	if (!locals.user) redirect(303, `/login?redirect=${encodeURIComponent(url.pathname)}`);
	return {};
};
