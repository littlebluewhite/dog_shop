<script lang="ts">
	import { cart } from '$lib/cart.svelte';
	import { priceRange, twd } from '$lib/format';
	import { toast } from '$lib/toast.svelte';
	import type { ProductDetail } from '$lib/types';

	let { product: p }: { product: ProductDetail } = $props();

	let sel1 = $state<string | null>(null);
	let sel2 = $state<string | null>(null);
	let qty = $state(1);
	let activeImage = $state(0);

	const opt1Values = $derived([
		...new Set(p.variants.map((v) => v.option1_value).filter((v): v is string => v !== null))
	]);
	const opt2Values = $derived([
		...new Set(
			p.variants
				.filter((v) => sel1 === null || v.option1_value === sel1)
				.map((v) => v.option2_value)
				.filter((v): v is string => v !== null)
		)
	]);
	const selected = $derived.by(() => {
		if (!p.option1_name) return p.variants[0] ?? null;
		if (sel1 === null) return null;
		if (p.option2_name && sel2 === null) return null;
		return p.variants.find((v) => v.option1_value === sel1 && (!p.option2_name || v.option2_value === sel2)) ?? null;
	});
	const prices = $derived(p.variants.map((v) => v.price));
	const minPrice = $derived(prices.length ? Math.min(...prices) : 0);
	const maxPrice = $derived(prices.length ? Math.max(...prices) : 0);
	const shownImage = $derived(selected?.image_path ?? p.images[activeImage]?.path ?? null);

	function isAvailable(o1: string, o2: string | null): boolean {
		return p.variants.some((v) => v.option1_value === o1 && (o2 === null || v.option2_value === o2) && v.stock > 0);
	}

	function addToCart() {
		if (!selected || selected.stock <= 0) return;
		cart.add(
			{
				variant_id: selected.id,
				product_slug: p.slug,
				product_name: p.name,
				variant_label: [selected.option1_value, selected.option2_value].filter(Boolean).join(' / ') || '預設',
				price: selected.price,
				image_thumb: p.images[0]?.thumb_path ?? null
			},
			Math.min(Math.max(1, Number(qty) || 1), selected.stock)
		);
		toast.show('已加入購物車');
	}
</script>

<nav class="text-sm text-gray-500">
	<a href="/products" class="hover:underline">全部商品</a>
	{#if p.category}
		› <a href={`/products?category=${p.category.slug}`} class="hover:underline">{p.category.name}</a>
	{/if}
</nav>

<div class="mt-4 grid gap-8 md:grid-cols-2">
	<div>
		<div class="aspect-square overflow-hidden rounded-lg bg-gray-100">
			{#if shownImage}<img src={shownImage} alt={p.name} class="h-full w-full object-cover" />{/if}
		</div>
		{#if p.images.length > 1}
			<div class="mt-2 flex gap-2 overflow-x-auto">
				{#each p.images as img, i (img.path)}
					<button
						type="button"
						class="h-16 w-16 shrink-0 overflow-hidden rounded border {i === activeImage ? 'border-gray-900' : 'border-gray-200'}"
						onclick={() => (activeImage = i)}
						aria-label={`第 ${i + 1} 張圖`}
					>
						<img src={img.thumb_path} alt={img.alt} class="h-full w-full object-cover" />
					</button>
				{/each}
			</div>
		{/if}
	</div>

	<div>
		<h1 class="text-2xl font-bold">{p.name}</h1>
		<div class="mt-3 text-2xl font-semibold">
			{#if selected}
				{twd(selected.price)}
				{#if selected.compare_at_price && selected.compare_at_price > selected.price}
					<span class="ml-2 text-base font-normal text-gray-400 line-through">{twd(selected.compare_at_price)}</span>
				{/if}
			{:else}
				{priceRange(minPrice, maxPrice)}
			{/if}
		</div>

		{#if p.option1_name}
			<div class="mt-5">
				<div class="text-sm text-gray-600">{p.option1_name}</div>
				<div class="mt-2 flex flex-wrap gap-2">
					{#each opt1Values as value (value)}
						<button
							type="button"
							class="rounded border px-3 py-1.5 text-sm {sel1 === value ? 'border-gray-900 bg-gray-900 text-white' : 'border-gray-300'} {isAvailable(value, null) ? '' : 'opacity-40'}"
							onclick={() => {
								sel1 = value;
								sel2 = null;
							}}
						>
							{value}
						</button>
					{/each}
				</div>
			</div>
		{/if}
		{#if p.option2_name && sel1 !== null}
			<div class="mt-4">
				<div class="text-sm text-gray-600">{p.option2_name}</div>
				<div class="mt-2 flex flex-wrap gap-2">
					{#each opt2Values as value (value)}
						<button
							type="button"
							class="rounded border px-3 py-1.5 text-sm {sel2 === value ? 'border-gray-900 bg-gray-900 text-white' : 'border-gray-300'} {isAvailable(sel1 ?? '', value) ? '' : 'opacity-40'}"
							onclick={() => (sel2 = value)}
						>
							{value}
						</button>
					{/each}
				</div>
			</div>
		{/if}

		<div class="mt-5 text-sm text-gray-600">
			{#if selected}
				{selected.stock > 0 ? `庫存 ${selected.stock}` : '已售完'}
			{:else if p.option1_name}
				請選擇規格
			{/if}
		</div>

		<div class="mt-4 flex items-center gap-3">
			<input type="number" min="1" max={selected?.stock ?? 99} bind:value={qty} class="w-20 rounded border border-gray-300 px-3 py-2" aria-label="數量" />
			<button
				type="button"
				onclick={addToCart}
				disabled={!selected || selected.stock <= 0}
				class="rounded bg-gray-900 px-6 py-2 text-white disabled:opacity-40"
			>
				加入購物車
			</button>
		</div>

		{#if p.description}
			<div class="mt-8">
				<h2 class="font-semibold">商品說明</h2>
				<p class="mt-2 text-sm whitespace-pre-line text-gray-700">{p.description}</p>
			</div>
		{/if}
	</div>
</div>
