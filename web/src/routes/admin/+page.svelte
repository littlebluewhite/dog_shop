<script lang="ts">
	import { formatDate, twd } from '$lib/format';
	import { ORDER_STATUS_LABELS } from '$lib/labels';
	import type { AdminOrderListItem } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const d = $derived(data.dashboard);

	const tiles = $derived([
		{ label: '今日訂單', value: String(d.today_orders), note: `已付款 ${twd(d.today_paid_total)}`, href: '/admin/orders' },
		{ label: '待出貨', value: String(d.pending_shipment), note: '已付款、還沒出貨', href: '/admin/orders?status=paid' },
		{ label: '發票開立失敗', value: String(d.invoice_failed), note: '到訂單頁重開', href: '/admin/orders?flag=invoice_failed' },
		{ label: '需退款', value: String(d.needs_refund), note: '遲到或金額不符的付款', href: '/admin/orders?flag=needs_refund' },
		{ label: '超商退回', value: String(d.cvs_returned), note: '買家未取件', href: '/admin/orders?flag=cvs_returned' }
	]);
</script>

<svelte:head><title>儀表板</title></svelte:head>

<h1 class="text-2xl font-bold">儀表板</h1>

<div class="mt-4 grid grid-cols-2 gap-3 md:grid-cols-5">
	{#each tiles as t (t.label)}
		<a href={t.href} class="rounded border border-gray-200 bg-white p-4 hover:border-gray-400">
			<div class="text-sm text-gray-500">{t.label}</div>
			<div class="text-2xl font-bold">{t.value}</div>
			<div class="text-xs text-gray-500">{t.note}</div>
		</a>
	{/each}
</div>

{#snippet list(title: string, items: AdminOrderListItem[], href: string)}
	<section class="rounded border border-gray-200 bg-white">
		<div class="flex items-center justify-between border-b border-gray-200 px-4 py-2">
			<h2 class="font-medium">{title}</h2>
			<a {href} class="text-sm underline">全部</a>
		</div>
		{#if items.length === 0}
			<p class="p-4 text-sm text-gray-500">沒有</p>
		{:else}
			<ul class="divide-y divide-gray-100 text-sm">
				{#each items as o (o.id)}
					<li class="flex items-center justify-between gap-2 px-4 py-2">
						<a href={`/admin/orders/${o.id}`} class="font-medium hover:underline">{o.order_no}</a>
						<span class="min-w-0 flex-1 truncate text-gray-600">{o.recipient_name}｜{o.item_count} 件｜{twd(o.total)}</span>
						<span class="text-gray-500">{ORDER_STATUS_LABELS[o.status]}</span>
						<span class="text-xs text-gray-500">{formatDate(o.created_at)}</span>
					</li>
				{/each}
			</ul>
		{/if}
	</section>
{/snippet}

<div class="mt-6 grid gap-4 lg:grid-cols-2">
	{@render list('待出貨', d.pending_shipment_items, '/admin/orders?status=paid')}
	{@render list('需退款', d.needs_refund_items, '/admin/orders?flag=needs_refund')}
	{@render list('超商退回', d.cvs_returned_items, '/admin/orders?flag=cvs_returned')}
	{@render list('發票開立失敗', d.invoice_failed_items, '/admin/orders?flag=invoice_failed')}
</div>
