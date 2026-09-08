import { describe, expect, it } from 'vitest';
import { canCommit, summarize } from './importPreview';
import type { ImportPreview } from './types';

function preview(overrides: Partial<ImportPreview> = {}): ImportPreview {
	return {
		fingerprint: 'abc',
		product_count: 2,
		variant_count: 3,
		new_count: 1,
		update_count: 1,
		parsed: { sheet: 's', header_row: 1, row_count: 3, products: [], errors: [], unmatched_columns: [] },
		...overrides
	};
}

describe('importPreview', () => {
	it('summarizes counts', () => {
		expect(summarize(preview())).toEqual({ line: '2 個商品、3 個規格（新增 1、更新 1）', errorCount: 0 });
	});
	it('blocks commit on errors or empty file', () => {
		expect(canCommit(preview())).toBe(true);
		expect(canCommit(preview({ parsed: { ...preview().parsed, errors: [{ row: 2, column: '價格', message: 'x' }] } }))).toBe(false);
		expect(canCommit(preview({ product_count: 0 }))).toBe(false);
	});
});
