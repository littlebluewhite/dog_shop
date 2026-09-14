<script lang="ts">
	import Badge from '$lib/components/ui/Badge.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
	import { priceRange } from '$lib/format';
	import type { ProductListItem } from '$lib/types';

	let { item }: { item: ProductListItem } = $props();
</script>

<!-- 沒有邊框、沒有陰影（規格 §5.3）：只有圖、名稱、價格 -->
<a href={`/products/${item.slug}`} class="block rounded-card">
	<div class="relative aspect-square overflow-hidden rounded-control bg-surface">
		{#if item.image_thumb}
			<img src={item.image_thumb} alt={item.name} class="h-full w-full object-cover {item.in_stock ? '' : 'opacity-60'}" loading="lazy" />
		{:else}
			<div class="flex h-full w-full items-center justify-center text-ink-soft"><Icon name="image" size={32} /></div>
		{/if}
		{#if !item.in_stock}<span class="absolute top-2 right-2"><Badge tone="danger">已售完</Badge></span>{/if}
	</div>
	<div class="px-1 pt-3 pb-1">
		<h3 class="line-clamp-2 text-sm leading-snug text-ink">{item.name}</h3>
		<p class="mt-1 text-base font-bold tabular-nums">{priceRange(item.price_min, item.price_max)}</p>
	</div>
</a>
