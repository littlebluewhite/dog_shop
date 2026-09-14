<script lang="ts">
	import { untrack } from 'svelte';
	import { goto, invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import { toast } from '$lib/toast.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import type { AdminProduct, Category, ProductStatus } from '$lib/types';

	type VariantForm = {
		id: string | null;
		option1_value: string;
		option2_value: string;
		sku: string;
		price: number;
		compare_at_price: number | null;
		stock: number;
		is_active: boolean;
		image_path: string | null;
	};
	type ImageForm = { path: string; thumb_path: string; alt: string };

	let { product = null, categories }: { product?: AdminProduct | null; categories: Category[] } = $props();

	function emptyVariant(): VariantForm {
		return {
			id: null,
			option1_value: '',
			option2_value: '',
			sku: '',
			price: 0,
			compare_at_price: null,
			stock: 0,
			is_active: true,
			image_path: null
		};
	}
	function imagePathById(id: string | null): string | null {
		return product?.images.find((i) => i.id === id)?.path ?? null;
	}

	// 初始值刻意只取一次（product prop 之後不會變；存檔後由 {#key} 讓整個表單重新掛載）
	let name = $state(untrack(() => product?.name ?? ''));
	let slug = $state(untrack(() => product?.slug ?? ''));
	let description = $state(untrack(() => product?.description ?? ''));
	let category_id = $state(untrack(() => product?.category_id ?? ''));
	let status = $state<ProductStatus>(untrack(() => product?.status ?? 'draft'));
	let option1_name = $state(untrack(() => product?.option1_name ?? ''));
	let option2_name = $state(untrack(() => product?.option2_name ?? ''));
	let sort_order = $state(untrack(() => product?.sort_order ?? 0));
	let images = $state<ImageForm[]>(
		untrack(() => product?.images.map((i) => ({ path: i.path, thumb_path: i.thumb_path, alt: i.alt })) ?? [])
	);
	let variants = $state<VariantForm[]>(
		untrack(
			() =>
				product?.variants.map((v) => ({
					id: v.id,
					option1_value: v.option1_value ?? '',
					option2_value: v.option2_value ?? '',
					sku: v.sku ?? '',
					price: v.price,
					compare_at_price: v.compare_at_price,
					stock: v.stock,
					is_active: v.is_active,
					image_path: imagePathById(v.image_id)
				})) ?? [emptyVariant()]
		)
	);
	let opt1Input = $state('');
	let opt2Input = $state('');
	let saving = $state(false);
	let uploading = $state(false);
	let confirmArchive = $state(false);
	let errors = $state<Record<string, string>>({});
	let dragIndex = $state<number | null>(null);

	const hasOpt1 = $derived(option1_name.trim() !== '');
	const hasOpt2 = $derived(hasOpt1 && option2_name.trim() !== '');

	function splitValues(input: string): string[] {
		return input
			.split(/[,，]/)
			.map((s) => s.trim())
			.filter(Boolean);
	}

	/** 依「口味：雞,牛」×「尺寸：S,L」產生所有組合；已存在的組合保留原本的價格與庫存 */
	function generateVariants() {
		const values1 = splitValues(opt1Input);
		const values2 = hasOpt2 ? splitValues(opt2Input) : [''];
		if (values1.length === 0) {
			toast.show('請先輸入規格 1 的選項');
			return;
		}
		const next: VariantForm[] = [];
		for (const a of values1) {
			for (const b of values2) {
				const existing = variants.find((v) => v.option1_value === a && v.option2_value === b);
				next.push(existing ?? { ...emptyVariant(), option1_value: a, option2_value: b });
			}
		}
		variants = next;
	}
	function addVariant() {
		variants.push(emptyVariant());
	}
	function removeVariant(i: number) {
		variants = variants.filter((_, j) => j !== i);
	}
	function variantError(i: number): string | null {
		const keys = [
			`variants.${i}`,
			`variants.${i}.price`,
			`variants.${i}.compare_at_price`,
			`variants.${i}.stock`,
			`variants.${i}.option1_value`,
			`variants.${i}.option2_value`,
			`variants.${i}.image_path`
		];
		for (const key of keys) if (errors[key]) return errors[key];
		return null;
	}

	async function onFiles(e: Event) {
		const input = e.currentTarget as HTMLInputElement;
		const files = Array.from(input.files ?? []);
		input.value = '';
		if (images.length + files.length > 9) {
			toast.show('圖片最多 9 張');
			return;
		}
		if (files.some((f) => f.size > 10 * 1024 * 1024)) {
			toast.show('圖片不能超過 10MB');
			return;
		}
		uploading = true;
		try {
			for (const file of files) {
				const form = new FormData();
				form.append('file', file);
				const stored = await api<{ path: string; thumb_path: string }>('/api/admin/uploads', {
					method: 'POST',
					body: form
				});
				images.push({ path: stored.path, thumb_path: stored.thumb_path, alt: '' });
			}
		} catch (err) {
			toast.show(err instanceof ApiError ? (err.field('file') ?? err.message) : '上傳失敗');
		} finally {
			uploading = false;
		}
	}
	function moveImage(from: number, to: number) {
		if (from === to) return;
		const next = [...images];
		const [moved] = next.splice(from, 1);
		next.splice(to, 0, moved);
		images = next;
	}
	function removeImage(i: number) {
		const removedPath = images[i].path;
		images = images.filter((_, j) => j !== i);
		for (const v of variants) if (v.image_path === removedPath) v.image_path = null;
	}

	function toNumberOrNull(value: unknown): number | null {
		if (value === null || value === undefined || value === '') return null;
		const n = Number(value);
		return Number.isFinite(n) ? n : null;
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		saving = true;
		errors = {};
		const body = {
			name,
			slug: slug.trim() || null,
			description,
			category_id: category_id || null,
			status,
			option1_name: hasOpt1 ? option1_name.trim() : null,
			option2_name: hasOpt2 ? option2_name.trim() : null,
			sort_order: Number(sort_order) || 0,
			images: images.map((i) => ({ path: i.path, thumb_path: i.thumb_path, alt: i.alt })),
			variants: variants.map((v) => ({
				id: v.id,
				option1_value: hasOpt1 ? v.option1_value : null,
				option2_value: hasOpt2 ? v.option2_value : null,
				sku: v.sku.trim() || null,
				price: Number(v.price) || 0,
				compare_at_price: toNumberOrNull(v.compare_at_price),
				stock: Number(v.stock) || 0,
				is_active: v.is_active,
				image_path: v.image_path
			}))
		};
		try {
			if (product) {
				await api(`/api/admin/products/${product.id}`, { method: 'PUT', body: JSON.stringify(body) });
				toast.show('已儲存');
				await invalidateAll();
			} else {
				const created = await api<AdminProduct>('/api/admin/products', {
					method: 'POST',
					body: JSON.stringify(body)
				});
				toast.show('已建立');
				await goto(`/admin/products/${created.id}`);
			}
		} catch (err) {
			if (err instanceof ApiError && err.code === 'VALIDATION') {
				errors = (err.details as { fields?: Record<string, string> } | null)?.fields ?? {};
				toast.show('有欄位沒填對，請看紅字');
			} else {
				toast.show(err instanceof ApiError ? err.message : '儲存失敗');
			}
		} finally {
			saving = false;
		}
	}

	async function archive() {
		if (!product) return;
		if (!confirmArchive) {
			confirmArchive = true;
			return;
		}
		try {
			await api(`/api/admin/products/${product.id}`, { method: 'DELETE' });
			toast.show('已下架封存');
			await goto('/admin/products');
		} catch (err) {
			toast.show(err instanceof ApiError ? err.message : '封存失敗');
		}
	}
</script>

<form onsubmit={save} class="space-y-6">
	<Card title="基本資料">
		<div class="grid gap-4 md:grid-cols-2">
			<Field class="md:col-span-2" label="商品名稱" bind:value={name} required error={errors.name} />
			<Field label="網址代稱（空白會自動產生）" bind:value={slug} placeholder="例如 chicken-food" error={errors.slug} />
			<Field label="分類" error={errors.category_id}>
				{#snippet children({ errorId, invalid })}
					<select bind:value={category_id} class="input mt-1" aria-invalid={invalid} aria-describedby={errorId}>
						<option value="">未分類</option>
						{#each categories as c (c.id)}
							<option value={c.id}>{c.name}</option>
						{/each}
					</select>
				{/snippet}
			</Field>
			<Field label="狀態">
				<select bind:value={status} class="input mt-1">
					<option value="draft">草稿（買家看不到）</option>
					<option value="active">上架</option>
					<option value="archived">已下架</option>
				</select>
			</Field>
			<Field label="排序（數字小的在前）" type="number" bind:value={sort_order} />
			<Field class="md:col-span-2" label="商品描述（純文字，會保留換行）">
				<textarea bind:value={description} rows="6" class="input mt-1"></textarea>
			</Field>
		</div>
	</Card>

	<Card title="圖片（最多 9 張；第一張是主圖；拖曳可以換順序）">
		{#if errors.images}<p class="field-error mb-2">{errors.images}</p>{/if}
		<ul class="flex flex-wrap gap-3">
			{#each images as img, i (img.path)}
				<li
					class="w-32 rounded-control border border-line p-1 {dragIndex === i ? 'opacity-50' : ''}"
					draggable="true"
					ondragstart={() => (dragIndex = i)}
					ondragover={(e) => e.preventDefault()}
					ondrop={(e) => {
						e.preventDefault();
						if (dragIndex !== null) moveImage(dragIndex, i);
						dragIndex = null;
					}}
					ondragend={() => (dragIndex = null)}
				>
					<img src={img.thumb_path} alt={img.alt} class="aspect-square w-full rounded-lg object-cover" />
					<input bind:value={img.alt} placeholder="圖片說明" class="input mt-1 px-2 py-1 text-xs" />
					<div class="mt-1 flex justify-between text-xs">
						<span class="text-ink-soft">{i === 0 ? '主圖' : `第 ${i + 1} 張`}</span>
						<button type="button" class="font-medium text-danger" onclick={() => removeImage(i)}>移除</button>
					</div>
				</li>
			{/each}
			{#if images.length < 9}
				<li>
					<label class="flex aspect-square w-32 cursor-pointer items-center justify-center rounded-control border-2 border-dashed border-line text-sm text-ink-soft hover:border-brand hover:bg-brand-soft/40">
						{uploading ? '上傳中…' : '＋ 加圖片'}
						<input
							type="file"
							accept="image/jpeg,image/png,image/webp,image/gif"
							multiple
							onchange={onFiles}
							disabled={uploading}
							class="hidden"
						/>
					</label>
				</li>
			{/if}
		</ul>
	</Card>

	<Card title="規格">
		<p class="text-sm text-ink-soft">
			沒有規格就留空，只會有一列預設規格。有規格就填名稱（例如「口味」「尺寸」），用逗號列出選項，按「依選項產生規格」。
		</p>
		{#if errors.variants}<p class="field-error">{errors.variants}</p>{/if}
		{#if errors.option2_name}<p class="field-error">{errors.option2_name}</p>{/if}
		<div class="mt-3 grid gap-4 md:grid-cols-2">
			<div class="flex gap-2">
				<Field class="w-1/3" label="規格 1 名稱" bind:value={option1_name} placeholder="口味" />
				<Field class="flex-1" label="選項（逗號分隔）" bind:value={opt1Input} placeholder="雞肉, 牛肉" />
			</div>
			<div class="flex gap-2">
				<Field class="w-1/3" label="規格 2 名稱" bind:value={option2_name} disabled={!hasOpt1} placeholder="尺寸" />
				<Field class="flex-1" label="選項（逗號分隔）" bind:value={opt2Input} disabled={!hasOpt2} placeholder="S, M, L" />
			</div>
		</div>
		{#if hasOpt1}
			<div class="mt-3 flex gap-2">
				<Button variant="secondary" size="sm" onclick={generateVariants}>依選項產生規格</Button>
				<Button variant="secondary" size="sm" onclick={addVariant}>手動加一列</Button>
			</div>
		{/if}

		<div class="-mx-4 mt-3 overflow-x-auto md:-mx-5">
			<table class="table">
				<thead>
					<tr>
						{#if hasOpt1}<th>{option1_name}</th>{/if}
						{#if hasOpt2}<th>{option2_name}</th>{/if}
						<th>售價</th>
						<th>原價（可空）</th>
						<th>庫存</th>
						<th>SKU</th>
						<th>圖</th>
						<th>啟用</th>
						<th></th>
					</tr>
				</thead>
				<tbody>
					{#each variants as v, i}
						<tr class={v.is_active ? '' : 'opacity-60'}>
							{#if hasOpt1}<td><input bind:value={v.option1_value} class="input w-24 py-1.5" /></td>{/if}
							{#if hasOpt2}<td><input bind:value={v.option2_value} class="input w-20 py-1.5" /></td>{/if}
							<td><input type="number" min="0" bind:value={v.price} class="input w-24 py-1.5" /></td>
							<td><input type="number" min="0" bind:value={v.compare_at_price} class="input w-24 py-1.5" /></td>
							<td><input type="number" min="0" bind:value={v.stock} class="input w-20 py-1.5" /></td>
							<td><input bind:value={v.sku} class="input w-28 py-1.5" /></td>
							<td>
								<select bind:value={v.image_path} class="input w-auto py-1.5">
									<option value={null}>—</option>
									{#each images as img, j (img.path)}
										<option value={img.path}>第 {j + 1} 張</option>
									{/each}
								</select>
							</td>
							<td class="text-center"><input type="checkbox" bind:checked={v.is_active} class="accent-brand" /></td>
							<td>
								<button type="button" class="font-medium text-danger disabled:opacity-30" onclick={() => removeVariant(i)} disabled={variants.length === 1}>刪</button>
							</td>
						</tr>
						{#if variantError(i)}
							<tr><td colspan="9" class="text-danger">{variantError(i)}</td></tr>
						{/if}
					{/each}
				</tbody>
			</table>
		</div>
	</Card>

	<div class="flex items-center gap-3">
		<Button type="submit" size="lg" disabled={saving || uploading}>
			{saving ? '儲存中…' : product ? '儲存' : '建立商品'}
		</Button>
		<a href="/admin/products" class="link text-sm">回列表</a>
		{#if product && product.status !== 'archived'}
			<Button variant={confirmArchive ? 'danger' : 'secondary'} class="ml-auto" onclick={archive}>
				{confirmArchive ? '確定下架封存？' : '下架封存'}
			</Button>
		{/if}
	</div>
</form>
