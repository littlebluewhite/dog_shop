import { describe, expect, it } from 'vitest';
import { ApiError, parseResponse } from './api';

describe('parseResponse', () => {
	it('returns parsed JSON on success', async () => {
		const res = new Response(JSON.stringify({ ok: 1 }), { status: 200 });
		await expect(parseResponse<{ ok: number }>(res)).resolves.toEqual({ ok: 1 });
	});

	it('returns undefined on 204', async () => {
		const res = new Response(null, { status: 204 });
		await expect(parseResponse(res)).resolves.toBeUndefined();
	});

	it('throws ApiError with code, message and field errors from the envelope', async () => {
		const body = {
			error: { code: 'VALIDATION', message: '輸入資料有誤', details: { fields: { name: '必填' } } }
		};
		const res = new Response(JSON.stringify(body), { status: 400 });
		const err = await parseResponse(res).catch((e: unknown) => e);
		expect(err).toBeInstanceOf(ApiError);
		const apiErr = err as ApiError;
		expect(apiErr.status).toBe(400);
		expect(apiErr.code).toBe('VALIDATION');
		expect(apiErr.message).toBe('輸入資料有誤');
		expect(apiErr.field('name')).toBe('必填');
		expect(apiErr.field('other')).toBeUndefined();
	});

	it('handles non-JSON error bodies', async () => {
		const res = new Response('Bad Gateway', { status: 502 });
		const err = await parseResponse(res).catch((e: unknown) => e);
		expect(err).toBeInstanceOf(ApiError);
		expect((err as ApiError).code).toBe('HTTP_502');
	});
});
