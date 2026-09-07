import { describe, expect, it } from 'vitest';
import { defaultForm, toOrderInput, validateForm, type CheckoutForm } from './checkout';

function homeForm(): CheckoutForm {
	const f = defaultForm(null, 'credit');
	f.email = 'a@b.co';
	f.recipient_name = '王小明';
	f.recipient_phone = '0912345678';
	f.address = { postal_code: '100', city: '臺北市', district: '中正區', street: '重慶南路一段 122 號' };
	return f;
}
const ctx = { subtotal: 500, hasStore: false, enabledPayments: ['credit', 'atm'] as const };

describe('checkout form', () => {
	it('valid home form has no errors', () => {
		expect(validateForm(homeForm(), { ...ctx, enabledPayments: [...ctx.enabledPayments] })).toEqual({});
	});

	it('cvs needs a chinese name, a store, and subtotal under the limit', () => {
		const f = homeForm();
		f.shipping_method = 'cvs';
		f.recipient_name = 'Amy';
		const errors = validateForm(f, { subtotal: 25000, hasStore: false, enabledPayments: ['credit'] });
		expect(errors.recipient_name).toContain('中文');
		expect(errors.cvs_store).toBe('請先選擇取貨門市');
		expect(errors.shipping_method).toContain('宅配');
		expect(errors['address.street']).toBeUndefined();
	});

	it('invoice rules and payment method', () => {
		const f = homeForm();
		f.invoice.type = 'company';
		f.invoice.tax_id = '12345678';
		f.payment_method = 'cvs_code';
		const errors = validateForm(f, { subtotal: 1, hasStore: false, enabledPayments: ['credit'] });
		expect(errors['invoice.tax_id']).toBe('統一編號格式不正確');
		expect(errors['invoice.title']).toBeDefined();
		expect(errors.payment_method).toBe('請選擇付款方式');
	});

	it('toOrderInput sends only the relevant invoice fields', () => {
		const f = homeForm();
		f.invoice = { type: 'personal', carrier_type: '3', carrier_num: '/abc+123', tax_id: '99', title: 'x', address: 'y', love_code: '1' };
		const body = toOrderInput(f, [{ variant_id: 'v1', qty: 2 }], 'tok');
		expect(body.invoice).toEqual({ type: 'personal', carrier_type: '3', carrier_num: '/ABC+123' });
		expect(body.cvs_store_token).toBeNull();
		expect(body.address?.city).toBe('臺北市');
		f.shipping_method = 'cvs';
		const cvs = toOrderInput(f, [], 'tok');
		expect(cvs.cvs_store_token).toBe('tok');
		expect(cvs.address).toBeNull();
	});

	it('toOrderInput drops carrier_num for the 綠界會員載具 carrier type', () => {
		const f = homeForm();
		f.invoice = { type: 'personal', carrier_type: '1', carrier_num: '/abc+123', tax_id: '', title: '', address: '', love_code: '' };
		const body = toOrderInput(f, [{ variant_id: 'v1', qty: 1 }], null);
		expect(body.invoice).toEqual({ type: 'personal', carrier_type: '1', carrier_num: undefined });
	});

	it('toOrderInput for a donation invoice sends only the love code', () => {
		const f = homeForm();
		f.invoice = { type: 'donation', carrier_type: '1', carrier_num: '', tax_id: '', title: '', address: '', love_code: '123' };
		const body = toOrderInput(f, [{ variant_id: 'v1', qty: 1 }], null);
		expect(body.invoice).toEqual({ type: 'donation', love_code: '123' });
	});
});
