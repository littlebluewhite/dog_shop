import { env } from '$env/dynamic/private';
import { parseResponse } from '$lib/api';

export { ApiError } from '$lib/api';

/**
 * 伺服器端（load、hooks、+server.ts）呼叫 API：用內網位址，並把瀏覽器的 cookie 轉送過去。
 * 只需要 event.request，所以 RequestEvent / ServerLoadEvent 都可以直接傳。
 */
export async function serverApi<T>(
	event: { request: Request },
	path: string,
	init: RequestInit = {}
): Promise<T> {
	const base = (env.API_INTERNAL_URL ?? 'http://localhost:8080').replace(/\/$/, '');
	const headers = new Headers(init.headers);
	headers.set('accept', 'application/json');
	const cookie = event.request.headers.get('cookie');
	if (cookie) headers.set('cookie', cookie);
	if (typeof init.body === 'string' && !headers.has('content-type')) {
		headers.set('content-type', 'application/json');
	}
	const res = await fetch(base + path, { ...init, headers });
	return parseResponse<T>(res);
}
