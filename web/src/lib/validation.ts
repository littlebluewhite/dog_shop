import type { ShippingMethod, ShippingSettings } from '$lib/types';

/** 超商取貨商品小計上限（綠界 C2C） */
export const CVS_SUBTOTAL_LIMIT = 20000;
export const MAX_QTY = 99;

/** 台灣手機：09 開頭共 10 碼 */
export function isTwMobile(phone: string): boolean {
	return /^09\d{8}$/.test(phone);
}

/** 郵遞區號 3～5 碼數字 */
export function isPostalCode(code: string): boolean {
	return /^\d{3,5}$/.test(code);
}

/** 超商取貨收件人：2～5 個中文字 */
export function isCvsRecipientName(name: string): boolean {
	return /^[一-鿿]{2,5}$/.test(name);
}

/** 手機條碼載具：/ 開頭 + 7 碼（0-9、A-Z、+、-、.） */
export function isMobileBarcode(s: string): boolean {
	return /^\/[0-9A-Z+\-.]{7}$/.test(s);
}

/** 自然人憑證條碼：2 個大寫英文字母 + 14 碼數字 */
export function isCitizenCert(s: string): boolean {
	return /^[A-Z]{2}\d{14}$/.test(s);
}

/** 捐贈愛心碼：3～7 碼數字 */
export function isLoveCode(s: string): boolean {
	return /^\d{3,7}$/.test(s);
}

/**
 * 統一編號（財政部規則）：各位數乘 1,2,1,2,1,2,4,1，每個乘積十位＋個位相加，
 * 總和能被 5 整除即合法；第 7 碼是 7 時總和 + 1 能被 5 整除也合法。全 0 不算。
 */
export function isTaxId(s: string): boolean {
	if (!/^\d{8}$/.test(s) || s === '00000000') return false;
	const weights = [1, 2, 1, 2, 1, 2, 4, 1];
	const digits = [...s].map(Number);
	const sum = digits.reduce((acc, d, i) => {
		const p = d * weights[i];
		return acc + Math.floor(p / 10) + (p % 10);
	}, 0);
	return sum % 5 === 0 || (digits[6] === 7 && (sum + 1) % 5 === 0);
}

/** 免運門檻 0 = 不免運；否則小計 >= 門檻免運（與後端 shipping_fee 相同） */
export function shippingFee(shipping: ShippingSettings, method: ShippingMethod, subtotal: number): number {
	if (shipping.free_threshold > 0 && subtotal >= shipping.free_threshold) return 0;
	return method === 'cvs' ? shipping.cvs_fee : shipping.home_fee;
}
