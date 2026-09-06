<script lang="ts">
	import Pagination from '$lib/components/Pagination.svelte';
	import ProductCard from '$lib/components/ProductCard.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const currentCategory = $derived(data.categories.find((c) => c.slug === data.category));
	const title = $derived(currentCategory ? currentCategory.name : '全部商品');
</script>

<svelte:head><title>{title}</title></svelte:head>

<h1 class="text-2xl font-bold">{title}</h1>

<form method="GET" class="mt-4 flex flex-wrap gap-2">
	<input name="q" value={data.q} placeholder="搜尋商品" class="min-w-0 flex-1 rounded border border-gray-300 px-3 py-2 text-sm" />
	<select name="category" value={data.category} class="rounded border border-gray-300 px-3 py-2 text-sm">
		<option value="">全部分類</option>
		{#each data.categories as c (c.id)}
			<option value={c.slug}>{c.name}</option>
		{/each}
	</select>
	<select name="sort" value={data.sort} class="rounded border border-gray-300 px-3 py-2 text-sm">
		<option value="newest">最新</option>
		<option value="price_asc">價格低到高</option>
		<option value="price_desc">價格高到低</option>
	</select>
	<button type="submit" class="rounded bg-gray-900 px-4 py-2 text-sm text-white">搜尋</button>
</form>

{#if data.result.items.length === 0}
	<p class="mt-8 text-center text-gray-500">沒有符合的商品</p>
{:else}
	<p class="mt-4 text-sm text-gray-500">共 {data.result.total} 件</p>
	<div class="mt-3 grid grid-cols-2 gap-3 md:grid-cols-4">
		{#each data.result.items as item (item.slug)}
			<ProductCard {item} />
		{/each}
	</div>
	<Pagination page={data.result.page} perPage={data.result.per_page} total={data.result.total} />
{/if}
