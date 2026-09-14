import { describe, expect, it } from 'vitest';
import { clampQty } from './qty';

describe('clampQty', () => {
	it('範圍內原樣回傳（會去掉小數）', () => {
		expect(clampQty(5, 1, 99, 1)).toBe(5);
		expect(clampQty(2.7, 1, 99, 1)).toBe(2);
	});
	it('低於 min 夾到 min、高於 max 夾到 max', () => {
		expect(clampQty(0, 1, 99, 1)).toBe(1);
		expect(clampQty(200, 1, 99, 1)).toBe(99);
	});
	it('購物車用 min=0：減到 0 要回 0（外面會把該列移除）', () => {
		expect(clampQty(0, 0, 99, 1)).toBe(0);
	});
	it('已售完：max=0 小於 min=1 時 min 優先，按加減都停在 1（不會顯示 0）', () => {
		expect(clampQty(2, 1, 0, 1)).toBe(1);
		expect(clampQty(0, 1, 0, 1)).toBe(1);
	});
	it('不是數字就回 fallback（不動原值）', () => {
		expect(clampQty(Number.NaN, 1, 99, 3)).toBe(3);
		expect(clampQty(Number.POSITIVE_INFINITY, 1, 99, 3)).toBe(3);
	});
});
