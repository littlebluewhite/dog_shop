import type {
	CvsSubType,
	HomeAddressInput,
	InvoiceInput,
	InvoiceType,
	OrderInput,
	PaymentMethod,
	ShippingMethod,
	User
} from '$lib/types';
import {
	CVS_SUBTOTAL_LIMIT,
	isCitizenCert,
	isCvsRecipientName,
	isLoveCode,
	isMobileBarcode,
	isPostalCode,
	isTaxId,
	isTwMobile
} from '$lib/validation';

export const CHECKOUT_STORAGE_KEY = 'dog_shop_checkout_v1';

export type InvoiceForm = {
	type: InvoiceType;
	carrier_type: '1' | '2' | '3';
	carrier_num: string;
	tax_id: string;
	title: string;
	address: string;
	love_code: string;
};

export type CheckoutForm = {
	email: string;
	recipient_name: string;
	recipient_phone: string;
	shipping_method: ShippingMethod;
	cvs_sub_type: CvsSubType;
	address: HomeAddressInput;
	invoice: InvoiceForm;
	payment_method: PaymentMethod;
	note: string;
};

export function defaultForm(user: User | null, firstPayment: PaymentMethod): CheckoutForm {
	return {
		email: user?.email ?? '',
		recipient_name: user?.name ?? '',
		recipient_phone: user?.phone ?? '',
		shipping_method: 'home',
		cvs_sub_type: 'UNIMARTC2C',
		address: { postal_code: '', city: '', district: '', street: '' },
		invoice: { type: 'personal', carrier_type: '1', carrier_num: '', tax_id: '', title: '', address: '', love_code: '' },
		payment_method: firstPayment,
		note: ''
	};
}

export type ValidateContext = {
	subtotal: number;
	hasStore: boolean;
	enabledPayments: PaymentMethod[];
};

/** 與後端 `validate_input` 相同的規則，錯誤 key 也相同（`address.street`、`invoice.tax_id`…） */
export function validateForm(form: CheckoutForm, ctx: ValidateContext): Record<string, string> {
	const errors: Record<string, string> = {};
	const email = form.email.trim();
	if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) errors.email = 'Email 格式不正確';
	if (!isTwMobile(form.recipient_phone.trim())) errors.recipient_phone = '手機格式：09 開頭共 10 碼';
	const name = form.recipient_name.trim();
	if (form.shipping_method === 'cvs') {
		if (!isCvsRecipientName(name)) errors.recipient_name = '超商取貨收件人請填 2～5 個中文字的本名';
		if (ctx.subtotal > CVS_SUBTOTAL_LIMIT) errors.shipping_method = '商品小計超過 20,000 元，請改用宅配';
		if (!ctx.hasStore) errors.cvs_store = '請先選擇取貨門市';
	} else {
		if (name.length === 0 || name.length > 20) errors.recipient_name = '必填，最多 20 字';
		const a = form.address;
		if (!isPostalCode(a.postal_code.trim())) errors['address.postal_code'] = '郵遞區號 3～5 碼數字';
		if (!a.city) errors['address.city'] = '請選縣市';
		if (!a.district) errors['address.district'] = '請選鄉鎮市區';
		const street = a.street.trim();
		if (street.length === 0 || street.length > 100) errors['address.street'] = '必填，最多 100 字';
	}
	const inv = form.invoice;
	if (inv.type === 'personal') {
		const num = inv.carrier_num.trim().toUpperCase();
		if (inv.carrier_type === '2' && !isCitizenCert(num)) errors['invoice.carrier_num'] = '自然人憑證條碼：2 個英文字母 + 14 碼數字';
		if (inv.carrier_type === '3' && !isMobileBarcode(num)) errors['invoice.carrier_num'] = '手機條碼：/ 開頭共 8 碼';
	} else if (inv.type === 'company') {
		if (!isTaxId(inv.tax_id.trim())) errors['invoice.tax_id'] = '統一編號格式不正確';
		const title = inv.title.trim();
		if (title.length === 0 || title.length > 60) errors['invoice.title'] = '必填，最多 60 字';
		const addr = inv.address.trim();
		if (addr.length === 0 || addr.length > 100) errors['invoice.address'] = '必填，最多 100 字';
	} else if (!isLoveCode(inv.love_code.trim())) {
		errors['invoice.love_code'] = '愛心碼 3～7 碼數字';
	}
	if (!ctx.enabledPayments.includes(form.payment_method)) errors.payment_method = '請選擇付款方式';
	if (form.note.length > 200) errors.note = '最多 200 字';
	return errors;
}

/** 組成 POST /api/orders 的 body（只送需要的發票欄位） */
export function toOrderInput(
	form: CheckoutForm,
	items: { variant_id: string; qty: number }[],
	storeToken: string | null
): OrderInput {
	const inv = form.invoice;
	const invoice: InvoiceInput =
		inv.type === 'personal'
			? {
					type: 'personal',
					carrier_type: inv.carrier_type,
					carrier_num: inv.carrier_type === '1' ? undefined : inv.carrier_num.trim().toUpperCase()
				}
			: inv.type === 'company'
				? { type: 'company', tax_id: inv.tax_id.trim(), title: inv.title.trim(), address: inv.address.trim() }
				: { type: 'donation', love_code: inv.love_code.trim() };
	return {
		items,
		email: form.email.trim(),
		recipient_name: form.recipient_name.trim(),
		recipient_phone: form.recipient_phone.trim(),
		shipping_method: form.shipping_method,
		cvs_store_token: form.shipping_method === 'cvs' ? storeToken : null,
		address: form.shipping_method === 'home' ? form.address : null,
		invoice,
		payment_method: form.payment_method,
		note: form.note.trim()
	};
}

/** 綠界地圖的 Device：手機 1、桌機 0（規格 §8.3） */
export function isMobileDevice(userAgent: string): boolean {
	return /Android|iPhone|iPad|iPod|Mobile/i.test(userAgent);
}

/** map-reply 失敗時 303 回 /checkout?store_error=… 的文案（與規格不同之處 37） */
export const STORE_ERROR_MESSAGES: Record<string, string> = {
	expired: '門市選擇已逾時（超過 1 小時），請重新選擇門市',
	invalid: '綠界回傳的門市資料不完整，請重新選擇門市',
	server: '暫時無法儲存門市，請稍後再試'
};

export function storeErrorMessage(code: string | null): string | null {
	if (!code) return null;
	return STORE_ERROR_MESSAGES[code] ?? '門市選擇失敗，請重新選擇門市';
}
