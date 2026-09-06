import type { Handle } from '@sveltejs/kit';
import { ApiError, serverApi } from '$lib/server/api';
import type { User } from '$lib/types';

/** 每個請求：有 sid cookie 就問 API 是誰，放進 locals.user（規格 §6.2） */
export const handle: Handle = async ({ event, resolve }) => {
	event.locals.user = null;
	if (event.cookies.get('sid')) {
		try {
			const res = await serverApi<{ user: User }>(event, '/api/auth/me');
			event.locals.user = res.user;
		} catch (e) {
			// 401 = session 過期或被登出，當沒登入；其他錯誤記 log 但頁面照常顯示
			if (!(e instanceof ApiError && e.status === 401)) console.error('auth/me failed', e);
		}
	}
	return resolve(event);
};
