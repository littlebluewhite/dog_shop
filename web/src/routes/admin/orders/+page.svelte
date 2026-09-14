<script lang="ts">
	import Button from '$lib/components/ui/Button.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import StatusBadge from '$lib/components/ui/StatusBadge.svelte';
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

<PageHeader title="訂單" />

<form method="GET" class="mt-5 grid grid-cols-2 gap-2 md:flex md:flex-wrap">
	<input name="q" value={data.q} placeholder="訂單編號、Email、收件人" class="input col-span-2 md:w-72" />
	<select name="status" value={data.status} class="input md:w-40">
		<option value="">全部狀態</option>
		{#each statuses as s (s)}
			<option value={s}>{ORDER_STATUS_LABELS[s]}</option>
		{/each}
	</select>
	<select name="flag" value={data.flag} class="input md:w-40">
		<option value="">全部</option>
		{#each flags as f (f)}
			<option value={f}>{ADMIN_FLAG_LABELS[f]}</option>
		{/each}
	</select>
	<Button type="submit" variant="secondary" class="col-span-2 md:col-span-1">篩選</Button>
</form>

<div class="card mt-4 overflow-x-auto p-0 md:p-0">
	<table class="table">
		<thead>
			<tr>
				<th>訂單</th>
				<th>時間</th>
				<th>買家</th>
				<th>取貨</th>
				<th>金額</th>
				<th>狀態</th>
				<th>提醒</th>
			</tr>
		</thead>
		<tbody>
			{#each data.result.items as o (o.id)}
				{@const tags = attention(o)}
				<tr class={tags.length > 0 ? 'bg-danger-soft/60' : ''}>
					<td><a href={`/admin/orders/${o.id}`} class="font-semibold hover:underline">{o.order_no}</a></td>
					<td class="text-ink-soft">{formatDate(o.created_at)}</td>
					<td>
						<div>{o.recipient_name}</div>
						<div class="text-xs text-ink-soft">{o.email}</div>
					</td>
					<td>
						<div>{o.shipping_method === 'cvs' ? '超商取貨' : '宅配'}</div>
						{#if o.shipment_status}<div class="text-xs text-ink-soft">{SHIPMENT_STATUS_LABELS[o.shipment_status]}</div>{/if}
					</td>
					<td class="tabular-nums">{twd(o.total)}<div class="text-xs text-ink-soft">{o.item_count} 件</div></td>
					<td>
						<StatusBadge status={o.status} />
						{#if o.invoice_status}<div class="mt-1 text-xs text-ink-soft">發票{INVOICE_STATUS_LABELS[o.invoice_status]}</div>{/if}
					</td>
					<td class="font-medium text-danger">{tags.join('、')}</td>
				</tr>
			{:else}
				<tr><td colspan="7" class="p-6 text-center text-ink-soft">沒有訂單</td></tr>
			{/each}
		</tbody>
	</table>
</div>
<Pagination page={data.result.page} perPage={data.result.per_page} total={data.result.total} />
