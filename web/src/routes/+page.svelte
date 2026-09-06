<script lang="ts">
	import ProductCard from '$lib/components/ProductCard.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<section>
	<h2 class="text-lg font-bold">分類</h2>
	<div class="mt-3 flex flex-wrap gap-2">
		<a href="/products" class="rounded-full border border-gray-300 bg-white px-4 py-1.5 text-sm">全部</a>
		{#each data.categories as c (c.id)}
			<a href={`/products?category=${c.slug}`} class="rounded-full border border-gray-300 bg-white px-4 py-1.5 text-sm">{c.name}</a>
		{/each}
	</div>
</section>

<section class="mt-8">
	<div class="flex items-baseline justify-between">
		<h2 class="text-lg font-bold">最新商品</h2>
		<a href="/products" class="text-sm underline">看全部</a>
	</div>
	{#if data.latest.length === 0}
		<p class="mt-4 text-gray-500">商品準備中，請稍後再來。</p>
	{:else}
		<div class="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
			{#each data.latest as item (item.slug)}
				<ProductCard {item} />
			{/each}
		</div>
	{/if}
</section>
