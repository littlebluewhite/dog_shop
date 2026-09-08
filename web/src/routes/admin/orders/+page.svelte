<script lang="ts">
	import Pagination from '$lib/components/Pagination.svelte';
	import { formatDate, twd } from '$lib/format';
	import { ADMIN_FLAG_LABELS, INVOICE_STATUS_LABELS, ORDER_STATUS_LABELS, SHIPMENT_STATUS_LABELS } from '$lib/labels';
	import type { AdminOrderFlag, AdminOrderListItem, OrderStatus } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const statuses = Object.keys(ORDER_STATUS_LABELS) as OrderStatus[];
	const flags = Object.keys(ADMIN_FLAG_LABELS) as AdminOrderFlag[];

	/** 要老闆注意的列標紅（規格 §4） */
	function attention(o: AdminOrderListItem): string[] {
		const tags: string[] = [];
		if (o.needs_refund) tags.push('需退款');
		if (o.status === 'shipped' && o.shipment_status === 'returned') tags.push('超商退回');
		if (o.invoice_status === 'failed') tags.push('發票失敗');
		return tags;
	}
</script>

<svelte:head><title>訂單管理</title></svelte:head>

<h1 class="text-2xl font-bold">訂單</h1>

<form method="GET" class="mt-4 flex flex-wrap gap-2">
	<input name="q" value={data.q} placeholder="訂單編號、Email、收件人" class="rounded border border-gray-300 px-3 py-2 text-sm" />
	<select name="status" value={data.status} class="rounded border border-gray-300 px-3 py-2 text-sm">
		<option value="">全部狀態</option>
		{#each statuses as s (s)}
			<option value={s}>{ORDER_STATUS_LABELS[s]}</option>
		{/each}
	</select>
	<select name="flag" value={data.flag} class="rounded border border-gray-300 px-3 py-2 text-sm">
		<option value="">全部</option>
		{#each flags as f (f)}
			<option value={f}>{ADMIN_FLAG_LABELS[f]}</option>
		{/each}
	</select>
	<button type="submit" class="rounded border border-gray-300 px-4 py-2 text-sm">篩選</button>
</form>

<div class="mt-4 overflow-x-auto">
	<table class="w-full bg-white text-sm">
		<thead>
			<tr class="border-b border-gray-200 text-left">
				<th class="p-2">訂單</th>
				<th class="p-2">時間</th>
				<th class="p-2">買家</th>
				<th class="p-2">取貨</th>
				<th class="p-2">金額</th>
				<th class="p-2">狀態</th>
				<th class="p-2">提醒</th>
			</tr>
		</thead>
		<tbody>
			{#each data.result.items as o (o.id)}
				{@const tags = attention(o)}
				<tr class="border-b border-gray-100 {tags.length > 0 ? 'bg-red-50' : ''}">
					<td class="p-2"><a href={`/admin/orders/${o.id}`} class="font-medium hover:underline">{o.order_no}</a></td>
					<td class="p-2 text-gray-500">{formatDate(o.created_at)}</td>
					<td class="p-2">
						<div>{o.recipient_name}</div>
						<div class="text-xs text-gray-500">{o.email}</div>
					</td>
					<td class="p-2">
						<div>{o.shipping_method === 'cvs' ? '超商取貨' : '宅配'}</div>
						{#if o.shipment_status}<div class="text-xs text-gray-500">{SHIPMENT_STATUS_LABELS[o.shipment_status]}</div>{/if}
					</td>
					<td class="p-2">{twd(o.total)}<div class="text-xs text-gray-500">{o.item_count} 件</div></td>
					<td class="p-2">
						<div>{ORDER_STATUS_LABELS[o.status]}</div>
						{#if o.invoice_status}<div class="text-xs text-gray-500">發票{INVOICE_STATUS_LABELS[o.invoice_status]}</div>{/if}
					</td>
					<td class="p-2 text-red-700">{tags.join('、')}</td>
				</tr>
			{:else}
				<tr><td colspan="7" class="p-6 text-center text-gray-500">沒有訂單</td></tr>
			{/each}
		</tbody>
	</table>
</div>

<Pagination page={data.result.page} perPage={data.result.per_page} total={data.result.total} />
