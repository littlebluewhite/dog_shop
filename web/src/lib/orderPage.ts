import type { OrderDetail } from '$lib/types';

/** 付款中每 3 秒重新載入，最多 2 分鐘（規格 §6.1） */
export const POLL_INTERVAL_MS = 3_000;
export const POLL_MAX_MS = 120_000;

/** ATM 帳號或超商代碼已經拿到了（PaymentInfoURL 已回） */
export function hasPaymentInfo(order: OrderDetail): boolean {
	const p = order.payment;
	return !!p && (!!p.atm_vaccount || !!p.cvs_payment_no);
}

/** 還在等綠界回呼：待付款、而且還沒有繳費資訊（規格 §7 第 8 點） */
export function needsPolling(order: OrderDetail): boolean {
	return order.status === 'pending_payment' && !hasPaymentInfo(order);
}

/** 繳費期限過了（用瀏覽器時間比） */
export function paymentExpired(order: OrderDetail, now: number = Date.now()): boolean {
	const e = order.payment?.expire_at;
	return !!e && new Date(e).getTime() < now;
}
