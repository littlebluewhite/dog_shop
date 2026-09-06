<script lang="ts">
	import { untrack } from 'svelte';
	import { goto, invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import { toast } from '$lib/toast.svelte';
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

<form onsubmit={save} class="space-y-8">
	<section class="rounded border border-gray-200 bg-white p-4">
		<h2 class="font-semibold">基本資料</h2>
		<div class="mt-3 grid gap-4 md:grid-cols-2">
			<label class="block text-sm md:col-span-2">
				商品名稱
				<input bind:value={name} required class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
				{#if errors.name}<span class="text-red-600">{errors.name}</span>{/if}
			</label>
			<label class="block text-sm">
				網址代稱（空白會自動產生）
				<input bind:value={slug} placeholder="例如 chicken-food" class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
				{#if errors.slug}<span class="text-red-600">{errors.slug}</span>{/if}
			</label>
			<label class="block text-sm">
				分類
				<select bind:value={category_id} class="mt-1 w-full rounded border border-gray-300 px-3 py-2">
					<option value="">未分類</option>
					{#each categories as c (c.id)}
						<option value={c.id}>{c.name}</option>
					{/each}
				</select>
				{#if errors.category_id}<span class="text-red-600">{errors.category_id}</span>{/if}
			</label>
			<label class="block text-sm">
				狀態
				<select bind:value={status} class="mt-1 w-full rounded border border-gray-300 px-3 py-2">
					<option value="draft">草稿（買家看不到）</option>
					<option value="active">上架</option>
					<option value="archived">已下架</option>
				</select>
			</label>
			<label class="block text-sm">
				排序（數字小的在前）
				<input type="number" bind:value={sort_order} class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
			</label>
			<label class="block text-sm md:col-span-2">
				商品描述（純文字，會保留換行）
				<textarea bind:value={description} rows="6" class="mt-1 w-full rounded border border-gray-300 px-3 py-2"></textarea>
			</label>
		</div>
	</section>

	<section class="rounded border border-gray-200 bg-white p-4">
		<h2 class="font-semibold">圖片（最多 9 張；第一張是主圖；拖曳可以換順序）</h2>
		{#if errors.images}<p class="text-sm text-red-600">{errors.images}</p>{/if}
		<ul class="mt-3 flex flex-wrap gap-3">
			{#each images as img, i (img.path)}
				<li
					class="w-32 rounded border border-gray-200 p-1 {dragIndex === i ? 'opacity-50' : ''}"
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
					<img src={img.thumb_path} alt={img.alt} class="aspect-square w-full rounded object-cover" />
					<input bind:value={img.alt} placeholder="圖片說明" class="mt-1 w-full rounded border border-gray-300 px-1 py-0.5 text-xs" />
					<div class="mt-1 flex justify-between text-xs">
						<span class="text-gray-500">{i === 0 ? '主圖' : `第 ${i + 1} 張`}</span>
						<button type="button" class="text-red-600" onclick={() => removeImage(i)}>移除</button>
					</div>
				</li>
			{/each}
			{#if images.length < 9}
				<li>
					<label class="flex aspect-square w-32 cursor-pointer items-center justify-center rounded border-2 border-dashed border-gray-300 text-sm text-gray-500">
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
	</section>

	<section class="rounded border border-gray-200 bg-white p-4">
		<h2 class="font-semibold">規格</h2>
		<p class="mt-1 text-sm text-gray-500">
			沒有規格就留空，只會有一列預設規格。有規格就填名稱（例如「口味」「尺寸」），用逗號列出選項，按「依選項產生規格」。
		</p>
		{#if errors.variants}<p class="text-sm text-red-600">{errors.variants}</p>{/if}
		{#if errors.option2_name}<p class="text-sm text-red-600">{errors.option2_name}</p>{/if}
		<div class="mt-3 grid gap-4 md:grid-cols-2">
			<div class="flex gap-2">
				<label class="block w-1/3 text-sm">
					規格 1 名稱
					<input bind:value={option1_name} placeholder="口味" class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
				</label>
				<label class="block flex-1 text-sm">
					選項（逗號分隔）
					<input bind:value={opt1Input} placeholder="雞肉, 牛肉" class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
				</label>
			</div>
			<div class="flex gap-2">
				<label class="block w-1/3 text-sm">
					規格 2 名稱
					<input
						bind:value={option2_name}
						disabled={!hasOpt1}
						placeholder="尺寸"
						class="mt-1 w-full rounded border border-gray-300 px-3 py-2 disabled:bg-gray-100"
					/>
				</label>
				<label class="block flex-1 text-sm">
					選項（逗號分隔）
					<input
						bind:value={opt2Input}
						disabled={!hasOpt2}
						placeholder="S, M, L"
						class="mt-1 w-full rounded border border-gray-300 px-3 py-2 disabled:bg-gray-100"
					/>
				</label>
			</div>
		</div>
		{#if hasOpt1}
			<div class="mt-3 flex gap-2">
				<button type="button" onclick={generateVariants} class="rounded border border-gray-300 px-3 py-1 text-sm">依選項產生規格</button>
				<button type="button" onclick={addVariant} class="rounded border border-gray-300 px-3 py-1 text-sm">手動加一列</button>
			</div>
		{/if}

		<div class="mt-3 overflow-x-auto">
			<table class="w-full text-sm">
				<thead>
					<tr class="border-b border-gray-200 text-left">
						{#if hasOpt1}<th class="p-2">{option1_name}</th>{/if}
						{#if hasOpt2}<th class="p-2">{option2_name}</th>{/if}
						<th class="p-2">售價</th>
						<th class="p-2">原價（可空）</th>
						<th class="p-2">庫存</th>
						<th class="p-2">SKU</th>
						<th class="p-2">圖</th>
						<th class="p-2">啟用</th>
						<th class="p-2"></th>
					</tr>
				</thead>
				<tbody>
					{#each variants as v, i}
						<tr class="border-b border-gray-100 {v.is_active ? '' : 'opacity-60'}">
							{#if hasOpt1}<td class="p-2"><input bind:value={v.option1_value} class="w-24 rounded border border-gray-300 px-2 py-1" /></td>{/if}
							{#if hasOpt2}<td class="p-2"><input bind:value={v.option2_value} class="w-20 rounded border border-gray-300 px-2 py-1" /></td>{/if}
							<td class="p-2"><input type="number" min="0" bind:value={v.price} class="w-24 rounded border border-gray-300 px-2 py-1" /></td>
							<td class="p-2"><input type="number" min="0" bind:value={v.compare_at_price} class="w-24 rounded border border-gray-300 px-2 py-1" /></td>
							<td class="p-2"><input type="number" min="0" bind:value={v.stock} class="w-20 rounded border border-gray-300 px-2 py-1" /></td>
							<td class="p-2"><input bind:value={v.sku} class="w-28 rounded border border-gray-300 px-2 py-1" /></td>
							<td class="p-2">
								<select bind:value={v.image_path} class="rounded border border-gray-300 px-2 py-1">
									<option value={null}>—</option>
									{#each images as img, j (img.path)}
										<option value={img.path}>第 {j + 1} 張</option>
									{/each}
								</select>
							</td>
							<td class="p-2 text-center"><input type="checkbox" bind:checked={v.is_active} /></td>
							<td class="p-2">
								<button type="button" class="text-red-600 disabled:opacity-30" onclick={() => removeVariant(i)} disabled={variants.length === 1}>刪</button>
							</td>
						</tr>
						{#if variantError(i)}
							<tr><td colspan="9" class="p-2 text-red-600">{variantError(i)}</td></tr>
						{/if}
					{/each}
				</tbody>
			</table>
		</div>
	</section>

	<div class="flex items-center gap-3">
		<button type="submit" disabled={saving || uploading} class="rounded bg-gray-900 px-6 py-2 text-white disabled:opacity-50">
			{saving ? '儲存中…' : product ? '儲存' : '建立商品'}
		</button>
		<a href="/admin/products" class="text-sm text-gray-600 hover:underline">回列表</a>
		{#if product && product.status !== 'archived'}
			<button type="button" onclick={archive} class="ml-auto rounded border border-red-300 px-4 py-2 text-sm text-red-700">
				{confirmArchive ? '確定下架封存？' : '下架封存'}
			</button>
		{/if}
	</div>
</form>
