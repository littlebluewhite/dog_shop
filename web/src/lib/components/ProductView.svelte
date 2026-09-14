<script lang="ts">
	import Button from '$lib/components/ui/Button.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
	import QtyStepper from '$lib/components/ui/QtyStepper.svelte';
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

<nav class="flex items-center gap-1 text-sm text-ink-soft" aria-label="麵包屑">
	<a href="/products" class="hover:text-ink hover:underline">全部商品</a>
	{#if p.category}
		<Icon name="chevron-right" size={14} />
		<a href={`/products?category=${p.category.slug}`} class="hover:text-ink hover:underline">{p.category.name}</a>
	{/if}
</nav>

<div class="mt-4 grid gap-8 md:grid-cols-2 md:gap-10">
	<div>
		<div class="aspect-square overflow-hidden rounded-control bg-surface">
			{#if shownImage}
				<img src={shownImage} alt={p.name} class="h-full w-full object-cover" />
			{:else}
				<div class="flex h-full w-full items-center justify-center text-ink-soft"><Icon name="image" size={40} /></div>
			{/if}
		</div>
		{#if p.images.length > 1}
			<div class="mt-3 flex gap-2 overflow-x-auto pb-1">
				{#each p.images as img, i (img.path)}
					<button
						type="button"
						class="h-16 w-16 shrink-0 overflow-hidden rounded-lg border-2 {i === activeImage ? 'border-brand' : 'border-line'}"
						onclick={() => (activeImage = i)}
						aria-label={`第 ${i + 1} 張圖`}
					>
						<img src={img.thumb_path} alt={img.alt} class="h-full w-full object-cover" />
					</button>
				{/each}
			</div>
		{/if}
	</div>

	<div class="flex flex-col">
		<h1 class="text-2xl font-extrabold tracking-tight md:text-[28px]">{p.name}</h1>
		<div class="mt-3 text-[28px] font-bold tabular-nums">
			{#if selected}
				{twd(selected.price)}
				{#if selected.compare_at_price && selected.compare_at_price > selected.price}
					<span class="ml-2 text-base font-normal text-ink-soft line-through">{twd(selected.compare_at_price)}</span>
				{/if}
			{:else}
				{priceRange(minPrice, maxPrice)}
			{/if}
		</div>

		{#if p.option1_name}
			<div class="mt-5">
				<div class="text-sm font-semibold">{p.option1_name}</div>
				<div class="mt-2 flex flex-wrap gap-2">
					{#each opt1Values as value (value)}
						<button
							type="button"
							class="chip {sel1 === value ? 'chip-selected' : 'chip-idle'} {isAvailable(value, null) ? '' : 'chip-off'}"
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
				<div class="text-sm font-semibold">{p.option2_name}</div>
				<div class="mt-2 flex flex-wrap gap-2">
					{#each opt2Values as value (value)}
						<button
							type="button"
							class="chip {sel2 === value ? 'chip-selected' : 'chip-idle'} {isAvailable(sel1 ?? '', value) ? '' : 'chip-off'}"
							onclick={() => (sel2 = value)}
						>
							{value}
						</button>
					{/each}
				</div>
			</div>
		{/if}

		<div class="mt-5 text-sm text-ink-soft">
			{#if selected}
				{selected.stock > 0 ? `庫存 ${selected.stock}` : '已售完'}
			{:else if p.option1_name}
				請選擇規格
			{/if}
		</div>

		<!-- 手機：黏在螢幕底部的動作列（規格 §5.5）；md 以上回到資訊欄裡。整頁只有這一顆「加入購物車」 -->
		<div
			class="sticky bottom-0 z-30 -mx-4 mt-4 flex items-center gap-3 border-t border-line bg-ground/95 px-4 py-3 shadow-[0_-8px_24px_rgba(22,39,74,.08)] backdrop-blur md:static md:mx-0 md:border-0 md:bg-transparent md:p-0 md:shadow-none md:backdrop-blur-none"
		>
			<QtyStepper bind:value={qty} max={selected?.stock ?? 99} />
			<Button size="lg" class="flex-1 md:flex-none" onclick={addToCart} disabled={!selected || selected.stock <= 0}>
				<Icon name="cart" />加入購物車
			</Button>
		</div>

		{#if p.description}
			<div class="mt-8">
				<h2 class="text-lg font-semibold">商品說明</h2>
				<p class="mt-2 max-w-[65ch] text-base leading-7 whitespace-pre-line text-ink">{p.description}</p>
			</div>
		{/if}
	</div>
</div>
