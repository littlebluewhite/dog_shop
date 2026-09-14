<script lang="ts">
	import ProductCard from '$lib/components/ProductCard.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<!-- 橘色看板（規格 §5.2）：整站唯一搶眼的地方 -->
<section class="relative overflow-hidden rounded-hero bg-brand px-5 py-8 md:px-10 md:py-12">
	<!-- 散落的小圓點、小方塊、吊牌：表示「很多小東西」（規格 §3.4）；手機只露右半邊 -->
	<svg
		class="pointer-events-none absolute top-0 right-0 h-full w-1/2 text-brand-light opacity-40 md:w-2/5"
		viewBox="0 0 400 300"
		preserveAspectRatio="xMaxYMid slice"
		fill="currentColor"
		aria-hidden="true"
	>
		<circle cx="330" cy="40" r="18" />
		<rect x="240" y="24" width="30" height="30" rx="9" />
		<circle cx="385" cy="130" r="28" />
		<rect x="290" y="150" width="24" height="24" rx="7" />
		<circle cx="215" cy="215" r="14" />
		<path d="M320 200h34l22 22-22 22h-34z" />
		<circle cx="270" cy="95" r="10" />
		<rect x="360" y="230" width="20" height="20" rx="6" />
		<circle cx="190" cy="60" r="8" />
		<rect x="150" y="260" width="16" height="16" rx="5" />
	</svg>
	<div class="relative max-w-2xl">
		<h1 class="text-4xl font-extrabold tracking-tight text-ink md:text-[2.75rem]">{data.shop.name}</h1>
		{#if data.shop.description}<p class="mt-3 max-w-prose text-lg text-ink/80">{data.shop.description}</p>{/if}
		<div class="mt-6 flex flex-wrap gap-2">
			<a href="/products" class="rounded-full bg-ground px-4 py-2 text-sm font-semibold text-ink transition-colors duration-150 hover:bg-brand-soft">全部</a>
			{#each data.categories as c (c.id)}
				<a href={`/products?category=${c.slug}`} class="rounded-full bg-ground px-4 py-2 text-sm font-semibold text-ink transition-colors duration-150 hover:bg-brand-soft">{c.name}</a>
			{/each}
		</div>
	</div>
</section>

<section class="mt-10">
	<div class="flex items-baseline justify-between">
		<h2 class="text-[22px] font-extrabold tracking-tight">最新商品</h2>
		<a href="/products" class="link text-sm">看全部</a>
	</div>
	{#if data.latest.length === 0}
		<div class="mt-4"><EmptyState message="商品準備中，請稍後再來。" /></div>
	{:else}
		<div class="mt-4 grid grid-cols-2 gap-4 md:grid-cols-3 lg:grid-cols-4">
			{#each data.latest as item (item.slug)}
				<ProductCard {item} />
			{/each}
		</div>
	{/if}
</section>
