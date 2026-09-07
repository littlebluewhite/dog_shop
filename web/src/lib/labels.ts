import type { CvsSubType, InvoiceType, OrderStatus, PaymentMethod } from '$lib/types';

export const ORDER_STATUS_LABELS: Record<OrderStatus, string> = {
	pending_payment: '待付款',
	paid: '已付款',
	shipped: '已出貨',
	completed: '已完成',
	cancelled: '已取消',
	refunded: '已退款'
};

export const CVS_LABELS: Record<CvsSubType, string> = {
	UNIMARTC2C: '7-ELEVEN',
	FAMIC2C: '全家',
	HILIFEC2C: '萊爾富'
};

export const PAYMENT_LABELS: Record<PaymentMethod, string> = {
	credit: '信用卡',
	atm: 'ATM 轉帳',
	cvs_code: '超商代碼繳費'
};

export const INVOICE_LABELS: Record<InvoiceType, string> = {
	personal: '個人（電子發票）',
	company: '公司（統一編號）',
	donation: '捐贈'
};
