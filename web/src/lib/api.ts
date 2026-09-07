/** API 回的錯誤：對應後端 { error: { code, message, details } } */
export class ApiError extends Error {
	constructor(
		public status: number,
		public code: string,
		message: string,
		public details: unknown = null
	) {
		super(message);
		this.name = 'ApiError';
	}

	/** 驗證錯誤時取某個欄位的訊息 */
	field(name: string): string | undefined {
		const fields = (this.details as { fields?: Record<string, string> } | null)?.fields;
		return fields?.[name];
	}

	/** 驗證錯誤的整張欄位表；不是欄位錯誤時回空物件 */
	fields(): Record<string, string> {
		return (this.details as { fields?: Record<string, string> } | null)?.fields ?? {};
	}
}

/** 把 fetch 的 Response 變成資料或丟 ApiError。204 回 undefined。 */
export async function parseResponse<T>(res: Response): Promise<T> {
	if (res.status === 204) return undefined as T;
	const text = await res.text();
	let data: unknown = null;
	try {
		data = text ? JSON.parse(text) : null;
	} catch {
		data = null;
	}
	if (!res.ok) {
		const err = (data as { error?: { code?: string; message?: string; details?: unknown } } | null)?.error;
		throw new ApiError(
			res.status,
			err?.code ?? `HTTP_${res.status}`,
			err?.message ?? `請求失敗（${res.status}）`,
			err?.details ?? null
		);
	}
	return data as T;
}

/**
 * 瀏覽器端呼叫 API（同源 /api/...，開發時由 Vite proxy 轉到 :8080）。
 * 非 GET 自動帶 X-Requested-With（後端 CSRF 檢查要求）；字串 body 當 JSON；FormData 交給瀏覽器自己設 content-type。
 */
export async function api<T>(path: string, init: RequestInit = {}): Promise<T> {
	const method = (init.method ?? 'GET').toUpperCase();
	const headers = new Headers(init.headers);
	headers.set('accept', 'application/json');
	if (method !== 'GET') headers.set('x-requested-with', 'fetch');
	if (typeof init.body === 'string' && !headers.has('content-type')) {
		headers.set('content-type', 'application/json');
	}
	const res = await fetch(path, { ...init, method, headers, credentials: 'same-origin' });
	return parseResponse<T>(res);
}
