import type { OrderStatus, ProductStatus } from '$lib/types';

export type BadgeTone = 'brand' | 'neutral' | 'success' | 'warning' | 'danger';

/** 訂單狀態 → 膠囊顏色（規格 §4 StatusBadge） */
export function orderStatusTone(status: OrderStatus): BadgeTone {
	switch (status) {
		case 'pending_payment':
			return 'warning';
		case 'paid':
		case 'completed':
			return 'success';
		case 'shipped':
			return 'brand';
		case 'cancelled':
		case 'refunded':
			return 'neutral';
	}
}

/** 後台商品狀態 → 膠囊顏色 */
export function productStatusTone(status: ProductStatus): BadgeTone {
	switch (status) {
		case 'active':
			return 'success';
		case 'draft':
			return 'warning';
		case 'archived':
			return 'neutral';
	}
}
