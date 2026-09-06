<script lang="ts">
	import Pagination from '$lib/components/Pagination.svelte';
	import { formatDate, priceRange } from '$lib/format';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const statusLabel: Record<string, string> = { draft: '草稿', active: '上架', archived: '已下架' };
</script>

<svelte:head><title>商品管理</title></svelte:head>

<div class="flex items-center justify-between">
	<h1 class="text-2xl font-bold">商品</h1>
	<a href="/admin/products/new" class="rounded bg-gray-900 px-4 py-2 text-sm text-white">新增商品</a>
</div>

<form method="GET" class="mt-4 flex flex-wrap gap-2">
	<input name="q" value={data.q} placeholder="搜尋名稱" class="rounded border border-gray-300 px-3 py-2 text-sm" />
	<select name="status" value={data.status} class="rounded border border-gray-300 px-3 py-2 text-sm">
		<option value="">全部（不含已下架）</option>
		<option value="draft">草稿</option>
		<option value="active">上架</option>
		<option value="archived">已下架</option>
	</select>
	<button type="submit" class="rounded border border-gray-300 px-4 py-2 text-sm">篩選</button>
</form>

<div class="mt-4 overflow-x-auto">
	<table class="w-full bg-white text-sm">
		<thead>
			<tr class="border-b border-gray-200 text-left">
				<th class="p-2">圖</th>
				<th class="p-2">名稱</th>
				<th class="p-2">狀態</th>
				<th class="p-2">價格</th>
				<th class="p-2">庫存</th>
				<th class="p-2">更新</th>
			</tr>
		</thead>
		<tbody>
			{#each data.result.items as item (item.id)}
				<tr class="border-b border-gray-100">
					<td class="p-2">
						{#if item.image_thumb}<img src={item.image_thumb} alt="" class="h-12 w-12 rounded object-cover" />{/if}
					</td>
					<td class="p-2">
						<a href={`/admin/products/${item.id}`} class="font-medium hover:underline">{item.name}</a>
						<div class="text-xs text-gray-500">{item.category_name ?? '未分類'} · /products/{item.slug}</div>
					</td>
					<td class="p-2">{statusLabel[item.status] ?? item.status}</td>
					<td class="p-2">
						{item.price_min === null || item.price_max === null ? '—' : priceRange(item.price_min, item.price_max)}
					</td>
					<td class="p-2">{item.stock_total}</td>
					<td class="p-2 text-gray-500">{formatDate(item.updated_at)}</td>
				</tr>
			{:else}
				<tr><td colspan="6" class="p-6 text-center text-gray-500">沒有商品</td></tr>
			{/each}
		</tbody>
	</table>
</div>

<Pagination page={data.result.page} perPage={data.result.per_page} total={data.result.total} />
