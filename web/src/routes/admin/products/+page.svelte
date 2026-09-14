<script lang="ts">
	import Badge from '$lib/components/ui/Badge.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import { productStatusTone } from '$lib/components/ui/status';
	import Pagination from '$lib/components/Pagination.svelte';
	import { formatDate, priceRange } from '$lib/format';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const statusLabel: Record<string, string> = { draft: '草稿', active: '上架', archived: '已下架' };
</script>

<svelte:head><title>商品管理</title></svelte:head>

<PageHeader title="商品">
	<Button href="/admin/products/new">新增商品</Button>
</PageHeader>

<form method="GET" class="mt-5 grid grid-cols-2 gap-2 md:flex md:flex-wrap">
	<input name="q" value={data.q} placeholder="搜尋名稱" class="input col-span-2 md:w-72" />
	<select name="status" value={data.status} class="input md:w-48">
		<option value="">全部（不含已下架）</option>
		<option value="draft">草稿</option>
		<option value="active">上架</option>
		<option value="archived">已下架</option>
	</select>
	<Button type="submit" variant="secondary">篩選</Button>
</form>

<div class="card mt-4 overflow-x-auto p-0 md:p-0">
	<table class="table">
		<thead>
			<tr>
				<th>圖</th>
				<th>名稱</th>
				<th>狀態</th>
				<th>價格</th>
				<th>庫存</th>
				<th>更新</th>
			</tr>
		</thead>
		<tbody>
			{#each data.result.items as item (item.id)}
				<tr>
					<td class="w-16">
						{#if item.image_thumb}<img src={item.image_thumb} alt="" class="h-12 w-12 rounded-lg object-cover" />{/if}
					</td>
					<td>
						<a href={`/admin/products/${item.id}`} class="font-semibold hover:underline">{item.name}</a>
						<div class="text-xs text-ink-soft">{item.category_name ?? '未分類'} · /products/{item.slug}</div>
					</td>
					<td><Badge tone={productStatusTone(item.status)}>{statusLabel[item.status] ?? item.status}</Badge></td>
					<td class="tabular-nums">
						{item.price_min === null || item.price_max === null ? '—' : priceRange(item.price_min, item.price_max)}
					</td>
					<td class="tabular-nums">{item.stock_total}</td>
					<td class="text-ink-soft">{formatDate(item.updated_at)}</td>
				</tr>
			{:else}
				<tr><td colspan="6" class="p-6 text-center text-ink-soft">沒有商品</td></tr>
			{/each}
		</tbody>
	</table>
</div>
<Pagination page={data.result.page} perPage={data.result.per_page} total={data.result.total} />
