import { redirect } from '@sveltejs/kit';
import type { LayoutServerLoad } from './$types';

/** /admin 底下全部要 admin；不是就去登入頁（規格 §6.2） */
export const load: LayoutServerLoad = ({ locals, url }) => {
	if (!locals.user || locals.user.role !== 'admin') {
		redirect(303, `/login?redirect=${encodeURIComponent(url.pathname)}`);
	}
	return {};
};
