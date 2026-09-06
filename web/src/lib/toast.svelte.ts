export type Toast = { id: number; message: string };

class ToastStore {
	items = $state<Toast[]>([]);
	private seq = 0;

	show(message: string, ms = 2500) {
		const id = ++this.seq;
		this.items.push({ id, message });
		setTimeout(() => this.dismiss(id), ms);
	}

	dismiss(id: number) {
		this.items = this.items.filter((t) => t.id !== id);
	}
}

export const toast = new ToastStore();
