<script lang="ts">
	import Pagination from '$lib/components/Pagination.svelte';
	import ProductCard from '$lib/components/ProductCard.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const currentCategory = $derived(data.categories.find((c) => c.slug === data.category));
	const title = $derived(currentCategory ? currentCategory.name : '全部商品');
</script>

<svelte:head><title>{title}</title></svelte:head>

<PageHeader {title} subtitle={`共 ${data.result.total} 件`} />

<!-- 原本的 GET 表單照舊；手機：搜尋一列、分類＋排序一列、按鈕一列；桌機：一列排完 -->
<form method="GET" class="mt-5 grid grid-cols-2 gap-2 md:grid-cols-[1fr_10rem_10rem_auto]">
	<label class="relative col-span-2 block md:col-span-1">
		<span class="sr-only">搜尋商品</span>
		<Icon name="search" class="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-ink-soft" />
		<input name="q" value={data.q} placeholder="搜尋商品" class="input pl-10" />
	</label>
	<select name="category" value={data.category} class="input">
		<option value="">全部分類</option>
		{#each data.categories as c (c.id)}
			<option value={c.slug}>{c.name}</option>
		{/each}
	</select>
	<select name="sort" value={data.sort} class="input">
		<option value="newest">最新</option>
		<option value="price_asc">價格低到高</option>
		<option value="price_desc">價格高到低</option>
	</select>
	<Button type="submit" class="col-span-2 md:col-span-1">搜尋</Button>
</form>

{#if data.result.items.length === 0}
	<div class="mt-6"><EmptyState message="沒有符合的商品" /></div>
{:else}
	<div class="mt-6 grid grid-cols-2 gap-4 md:grid-cols-3 lg:grid-cols-4">
		{#each data.result.items as item (item.slug)}
			<ProductCard {item} />
		{/each}
	</div>
	<Pagination page={data.result.page} perPage={data.result.per_page} total={data.result.total} />
{/if}
