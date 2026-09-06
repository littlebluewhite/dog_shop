export function twd(amount: number): string {
	return 'NT$' + amount.toLocaleString('zh-TW');
}

export function priceRange(min: number, max: number): string {
	return min === max ? twd(min) : `${twd(min)} ～ ${twd(max)}`;
}

/** 顯示台北時間（規格 §8：DB 存 UTC、前端顯示台北時間） */
export function formatDate(iso: string): string {
	return new Date(iso).toLocaleString('zh-TW', { timeZone: 'Asia/Taipei', hour12: false });
}
