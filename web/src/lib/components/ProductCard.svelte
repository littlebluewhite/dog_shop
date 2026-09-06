<script lang="ts">
	import { priceRange } from '$lib/format';
	import type { ProductListItem } from '$lib/types';

	let { item }: { item: ProductListItem } = $props();
</script>

<a href={`/products/${item.slug}`} class="block overflow-hidden rounded-lg border border-gray-200 bg-white">
	<div class="aspect-square bg-gray-100">
		{#if item.image_thumb}
			<img src={item.image_thumb} alt={item.name} class="h-full w-full object-cover" loading="lazy" />
		{/if}
	</div>
	<div class="p-3">
		<h3 class="line-clamp-2 text-sm">{item.name}</h3>
		<p class="mt-1 font-semibold">{priceRange(item.price_min, item.price_max)}</p>
		{#if !item.in_stock}<p class="text-xs text-red-600">已售完</p>{/if}
	</div>
</a>
