import type { AdminOrderDetail } from '$lib/types';

export type AdminAction =
	| 'ship_cvs'
	| 'ship_home'
	| 'print_label'
	| 'complete'
	| 'cancel'
	| 'mark_refunded'
	| 'retry_invoice'
	| 'clear_refund';

/** 依訂單狀態決定哪些按鈕可用（規格 §4、§6.1；與後端各路由的狀態檢查一致） */
export function availableActions(o: AdminOrderDetail): AdminAction[] {
	const actions: AdminAction[] = [];
	if (o.status === 'paid' && o.shipping_method === 'cvs') actions.push('ship_cvs');
	if (o.status === 'paid' && o.shipping_method === 'home') actions.push('ship_home');
	if (o.shipping_method === 'cvs' && o.shipment?.ecpay_logistics_id && o.shipment.cvs_payment_no) actions.push('print_label');
	if (o.status === 'shipped') actions.push('complete');
	if (o.status === 'pending_payment') actions.push('cancel');
	if (o.status === 'paid' || o.status === 'shipped') actions.push('mark_refunded');
	if (o.invoice?.status === 'failed') actions.push('retry_invoice');
	if (o.needs_refund) actions.push('clear_refund');
	return actions;
}

/** 需要老闆注意的提醒（列表標紅、明細頁上方；規格 §4） */
export function attentionNotes(o: AdminOrderDetail): string[] {
	const notes: string[] = [];
	if (o.needs_refund) notes.push('有一筆遲到或金額不符的付款：請到綠界後台退款後按「已處理退款」');
	if (o.status === 'shipped' && o.shipment?.status === 'returned') notes.push('超商未取件已退回：請到綠界後台退款後按「標記已退款」');
	const code = o.shipment?.last_status_code;
	if (code === 'create_failed' || code === 'create_error') notes.push(`上次建立物流單失敗：${o.shipment?.last_status_msg ?? '請稍後再試'}`);
	if (o.invoice?.status === 'failed') notes.push(`發票開立失敗：${o.invoice.error ?? ''}`);
	if (o.status === 'refunded' && o.invoice?.status === 'issued') notes.push('已退款但發票已開立：請至綠界後台作廢發票');
	return notes;
}
