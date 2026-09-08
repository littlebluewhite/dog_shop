import { describe, expect, it } from 'vitest';
import { hasPaymentInfo, needsPolling, paymentExpired } from './orderPage';
import type { OrderDetail, OrderPayment } from './types';

function order(status: OrderDetail['status'], payment: Partial<OrderPayment> | null): OrderDetail {
	const base: OrderDetail = {
		id: 'o',
		order_no: 'DS260906ABCD',
		status,
		email: 'a@b.co',
		recipient_name: '王小明',
		recipient_phone: '0912345678',
		shipping_method: 'home',
		subtotal: 300,
		shipping_fee: 100,
		total: 400,
		note: '',
		invoice_type: 'personal',
		invoice_carrier_type: '1',
		invoice_carrier_num: null,
		invoice_tax_id: null,
		invoice_title: null,
		invoice_address: null,
		invoice_love_code: null,
		created_at: '2026-09-06T00:00:00Z',
		paid_at: null,
		shipped_at: null,
		completed_at: null,
		cancelled_at: null,
		cancel_reason: null,
		items: [],
		shipment: null,
		payment: null,
		invoice: null
	};
	return {
		...base,
		payment: payment
			? {
					method: 'credit',
					status: 'pending',
					amount: 400,
					atm_bank_code: null,
					atm_vaccount: null,
					cvs_payment_no: null,
					expire_at: null,
					...payment
				}
			: null
	};
}

describe('orderPage', () => {
	it('hasPaymentInfo 看 ATM 帳號或超商代碼', () => {
		expect(hasPaymentInfo(order('pending_payment', null))).toBe(false);
		expect(hasPaymentInfo(order('pending_payment', {}))).toBe(false);
		expect(hasPaymentInfo(order('pending_payment', { atm_vaccount: '1234567890123456' }))).toBe(true);
		expect(hasPaymentInfo(order('pending_payment', { cvs_payment_no: 'LLL26090612345' }))).toBe(true);
	});

	it('待付款且沒繳費資訊才輪詢', () => {
		expect(needsPolling(order('pending_payment', null))).toBe(true);
		expect(needsPolling(order('pending_payment', {}))).toBe(true);
		expect(needsPolling(order('pending_payment', { atm_vaccount: '123' }))).toBe(false);
		expect(needsPolling(order('paid', {}))).toBe(false);
		expect(needsPolling(order('cancelled', null))).toBe(false);
	});

	it('paymentExpired 用期限比現在', () => {
		const now = Date.parse('2026-09-06T00:00:00Z');
		expect(paymentExpired(order('pending_payment', { expire_at: '2026-09-05T23:59:59Z' }), now)).toBe(true);
		expect(paymentExpired(order('pending_payment', { expire_at: '2026-09-06T00:00:01Z' }), now)).toBe(false);
		expect(paymentExpired(order('pending_payment', {}), now)).toBe(false);
		expect(paymentExpired(order('pending_payment', null), now)).toBe(false);
	});
});
