<script lang="ts">
	import Pagination from '$lib/components/Pagination.svelte';
	import { formatDate, twd } from '$lib/format';
	import { ORDER_STATUS_LABELS } from '$lib/labels';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head><title>我的訂單</title></svelte:head>

<h1 class="text-2xl font-bold">我的訂單</h1>

{#if data.result.items.length === 0}
	<p class="mt-4 text-gray-600">還沒有訂單。<a href="/products" class="underline">去逛逛</a></p>
{:else}
	<div class="mt-4 overflow-x-auto rounded border border-gray-200 bg-white">
		<table class="w-full text-sm">
			<thead class="bg-gray-50 text-left text-gray-600">
				<tr><th class="p-3">訂單編號</th><th class="p-3">日期</th><th class="p-3">狀態</th><th class="p-3">件數</th><th class="p-3 text-right">金額</th></tr>
			</thead>
			<tbody>
				{#each data.result.items as o (o.id)}
					<tr class="border-t border-gray-100">
						<td class="p-3"><a href={`/orders/${o.id}`} class="font-medium underline">{o.order_no}</a></td>
						<td class="p-3 text-gray-600">{formatDate(o.created_at)}</td>
						<td class="p-3">{ORDER_STATUS_LABELS[o.status]}</td>
						<td class="p-3">{o.item_count}</td>
						<td class="p-3 text-right">{twd(o.total)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
	<Pagination page={data.result.page} perPage={data.result.per_page} total={data.result.total} />
{/if}
