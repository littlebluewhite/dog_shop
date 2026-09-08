import { describe, expect, it } from 'vitest';
import { attentionNotes, availableActions } from './adminOrders';
import type { AdminOrderDetail, AdminShipment } from './types';

function shipment(over: Partial<AdminShipment> = {}): AdminShipment {
	return {
		id: 's1',
		order_id: 'o1',
		method: 'cvs',
		cvs_sub_type: 'UNIMARTC2C',
		cvs_store_id: '131386',
		cvs_store_name: '測試門市',
		cvs_store_address: '台北市',
		cvs_store_phone: null,
		home_postal_code: null,
		home_city: null,
		home_district: null,
		home_street: null,
		status: 'pending',
		ecpay_logistics_id: null,
		ecpay_merchant_trade_no: null,
		cvs_payment_no: null,
		cvs_validation_no: null,
		carrier: null,
		tracking_no: null,
		last_status_code: null,
		last_status_msg: null,
		created_at: '2026-09-08T00:00:00Z',
		updated_at: '2026-09-08T00:00:00Z',
		...over
	};
}

function order(over: Partial<AdminOrderDetail> = {}): AdminOrderDetail {
	return {
		id: 'o1',
		order_no: 'DS260908ABCD',
		status: 'paid',
		email: 'a@b.tw',
		recipient_name: '王小明',
		recipient_phone: '0912345678',
		shipping_method: 'cvs',
		subtotal: 600,
		shipping_fee: 60,
		total: 660,
		note: '',
		invoice_type: 'personal',
		invoice_carrier_type: '1',
		invoice_carrier_num: null,
		invoice_tax_id: null,
		invoice_title: null,
		invoice_address: null,
		invoice_love_code: null,
		created_at: '2026-09-08T00:00:00Z',
		paid_at: '2026-09-08T00:00:00Z',
		shipped_at: null,
		completed_at: null,
		cancelled_at: null,
		cancel_reason: null,
		items: [],
		user_id: null,
		needs_refund: false,
		shipment: shipment(),
		payments: [],
		invoice: { status: 'pending', invoice_no: null, invoice_date: null, random_number: null, error: null, updated_at: '2026-09-08T00:00:00Z' },
		...over
	};
}

describe('availableActions', () => {
	it('已付款超商：建物流單、標記退款；宅配：填單號', () => {
		expect(availableActions(order())).toEqual(['ship_cvs', 'mark_refunded']);
		expect(availableActions(order({ shipping_method: 'home', shipment: shipment({ method: 'home' }) }))).toEqual(['ship_home', 'mark_refunded']);
	});
	it('已出貨超商：列印、完成、標記退款', () => {
		const o = order({ status: 'shipped', shipment: shipment({ status: 'created', ecpay_logistics_id: '10035', cvs_payment_no: 'F1' }) });
		expect(availableActions(o)).toEqual(['print_label', 'complete', 'mark_refunded']);
	});
	it('待付款只能取消；完成／取消／退款沒有動作', () => {
		expect(availableActions(order({ status: 'pending_payment' }))).toEqual(['cancel']);
		expect(availableActions(order({ status: 'completed' }))).toEqual([]);
		expect(availableActions(order({ status: 'cancelled' }))).toEqual([]);
		expect(availableActions(order({ status: 'refunded' }))).toEqual([]);
	});
	it('發票失敗可重開、需退款可清除', () => {
		const o = order({ status: 'completed', needs_refund: true, invoice: { status: 'failed', invoice_no: null, invoice_date: null, random_number: null, error: 'x', updated_at: '' } });
		expect(availableActions(o)).toEqual(['retry_invoice', 'clear_refund']);
	});
});

describe('attentionNotes', () => {
	it('正常訂單沒有提醒', () => {
		expect(attentionNotes(order())).toEqual([]);
	});
	it('需退款、超商退回、建單失敗、發票失敗、退款後發票已開立', () => {
		const o = order({
			status: 'shipped',
			needs_refund: true,
			shipment: shipment({ status: 'returned', last_status_code: 'create_failed', last_status_msg: '收件人姓名格式錯誤' }),
			invoice: { status: 'failed', invoice_no: null, invoice_date: null, random_number: null, error: '綠界回錯', updated_at: '' }
		});
		const notes = attentionNotes(o);
		expect(notes.some((n) => n.includes('已處理退款'))).toBe(true);
		expect(notes.some((n) => n.includes('超商未取件'))).toBe(true);
		expect(notes.some((n) => n.includes('收件人姓名格式錯誤'))).toBe(true);
		expect(notes.some((n) => n.includes('綠界回錯'))).toBe(true);
		const refunded = order({ status: 'refunded', invoice: { status: 'issued', invoice_no: 'AB1', invoice_date: null, random_number: '1', error: null, updated_at: '' } });
		expect(attentionNotes(refunded).some((n) => n.includes('作廢發票'))).toBe(true);
	});
});
