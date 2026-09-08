import type { ImportPreview } from './types';

/** 有錯誤或沒有商品就不能匯入 */
export function canCommit(preview: ImportPreview): boolean {
	return preview.parsed.errors.length === 0 && preview.product_count > 0;
}

/** 一行摘要給預覽頁頂端 */
export function summarize(preview: ImportPreview): { line: string; errorCount: number } {
	const errorCount = preview.parsed.errors.length;
	const line = `${preview.product_count} 個商品、${preview.variant_count} 個規格（新增 ${preview.new_count}、更新 ${preview.update_count}）`;
	return { line, errorCount };
}
