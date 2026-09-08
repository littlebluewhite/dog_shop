import type { AdminOrderFlag, CvsSubType, InvoiceStatus, InvoiceType, OrderStatus, PaymentMethod, PaymentStatus, ShipmentStatus } from '$lib/types';

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

export const SHIPMENT_STATUS_LABELS: Record<ShipmentStatus, string> = {
	pending: '未出貨',
	created: '物流單已建立',
	in_transit: '運送中',
	arrived: '已到門市',
	picked_up: '已取件',
	returned: '未取退回',
	shipped: '已寄出'
};
export const PAYMENT_STATUS_LABELS: Record<PaymentStatus, string> = {
	pending: '等待付款',
	paid: '已付款',
	failed: '失敗',
	expired: '已作廢'
};
export const INVOICE_STATUS_LABELS: Record<InvoiceStatus, string> = {
	pending: '開立中',
	issued: '已開立',
	failed: '開立失敗'
};
export const ADMIN_FLAG_LABELS: Record<AdminOrderFlag, string> = {
	needs_refund: '需退款',
	cvs_returned: '超商退回',
	invoice_failed: '發票失敗'
};
