/**
 * 把數量夾在 [min, max]，去掉小數；不是有限數字就回 fallback（呼叫端會傳目前的值，等於不動）。
 * 先夾 max 再夾 min：商品已售完時 max=0 < min=1，min 要贏（停在 1），數量才不會顯示 0。
 */
export function clampQty(next: number, min: number, max: number, fallback: number): number {
	if (!Number.isFinite(next)) return fallback;
	return Math.max(Math.min(Math.trunc(next), max), min);
}
