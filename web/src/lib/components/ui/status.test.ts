import { describe, expect, it } from 'vitest';
import { orderStatusTone, productStatusTone } from './status';

describe('orderStatusTone（規格 §4 StatusBadge）', () => {
	it('六個訂單狀態各對到規格說的 tone', () => {
		expect(orderStatusTone('pending_payment')).toBe('warning');
		expect(orderStatusTone('paid')).toBe('success');
		expect(orderStatusTone('shipped')).toBe('brand');
		expect(orderStatusTone('completed')).toBe('success');
		expect(orderStatusTone('cancelled')).toBe('neutral');
		expect(orderStatusTone('refunded')).toBe('neutral');
	});
});

describe('productStatusTone（後台商品狀態）', () => {
	it('上架綠、草稿黃、下架灰', () => {
		expect(productStatusTone('active')).toBe('success');
		expect(productStatusTone('draft')).toBe('warning');
		expect(productStatusTone('archived')).toBe('neutral');
	});
});
