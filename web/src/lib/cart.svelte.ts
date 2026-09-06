export type CartLine = {
	variant_id: string;
	product_slug: string;
	product_name: string;
	variant_label: string;
	price: number;
	image_thumb: string | null;
	qty: number;
};

const STORAGE_KEY = 'dog_shop_cart_v1';
export const MAX_QTY = 99;

function readStorage(): CartLine[] {
	try {
		const raw = globalThis.localStorage?.getItem(STORAGE_KEY);
		if (!raw) return [];
		const parsed: unknown = JSON.parse(raw);
		if (!Array.isArray(parsed)) return [];
		return (parsed as CartLine[]).filter((l) => typeof l.variant_id === 'string' && Number.isFinite(l.qty) && l.qty > 0);
	} catch {
		return [];
	}
}

/** 購物車：Svelte 5 runes 類別，內容存 localStorage；價格與庫存只是顯示用快取，結帳時伺服器會重算 */
export class Cart {
	lines = $state<CartLine[]>([]);
	loaded = $state(false);
	count = $derived(this.lines.reduce((sum, l) => sum + l.qty, 0));
	subtotal = $derived(this.lines.reduce((sum, l) => sum + l.qty * l.price, 0));

	/** 在瀏覽器（layout 的 onMount）呼叫，把 localStorage 讀進來 */
	load() {
		this.lines = readStorage();
		this.loaded = true;
	}

	private persist() {
		try {
			globalThis.localStorage?.setItem(STORAGE_KEY, JSON.stringify(this.lines));
		} catch {
			// 無痕模式或空間滿：忽略，購物車只留在記憶體
		}
	}

	add(line: Omit<CartLine, 'qty'>, qty = 1) {
		const wanted = Math.max(1, Math.floor(qty));
		const existing = this.lines.find((l) => l.variant_id === line.variant_id);
		if (existing) {
			existing.qty = Math.min(MAX_QTY, existing.qty + wanted);
			Object.assign(existing, line); // 顯示用資料以最新為準
		} else {
			this.lines.push({ ...line, qty: Math.min(MAX_QTY, wanted) });
		}
		this.persist();
	}

	setQty(variant_id: string, qty: number) {
		if (!Number.isFinite(qty) || qty <= 0) {
			this.remove(variant_id);
			return;
		}
		const line = this.lines.find((l) => l.variant_id === variant_id);
		if (!line) return;
		line.qty = Math.min(MAX_QTY, Math.floor(qty));
		this.persist();
	}

	remove(variant_id: string) {
		this.lines = this.lines.filter((l) => l.variant_id !== variant_id);
		this.persist();
	}

	clear() {
		this.lines = [];
		this.persist();
	}
}

export const cart = new Cart();
