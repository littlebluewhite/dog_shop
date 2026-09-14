<script lang="ts">
	import Card from '$lib/components/ui/Card.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import StatusBadge from '$lib/components/ui/StatusBadge.svelte';
	import { formatDate, twd } from '$lib/format';
	import type { AdminOrderListItem } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const d = $derived(data.dashboard);

	const tiles = $derived([
		{ label: '今日訂單', value: String(d.today_orders), note: `已付款 ${twd(d.today_paid_total)}`, href: '/admin/orders', alert: false },
		{ label: '待出貨', value: String(d.pending_shipment), note: '已付款、還沒出貨', href: '/admin/orders?status=paid', alert: false },
		{ label: '發票開立失敗', value: String(d.invoice_failed), note: '到訂單頁重開', href: '/admin/orders?flag=invoice_failed', alert: true },
		{ label: '需退款', value: String(d.needs_refund), note: '遲到或金額不符的付款', href: '/admin/orders?flag=needs_refund', alert: true },
		{ label: '超商退回', value: String(d.cvs_returned), note: '買家未取件', href: '/admin/orders?flag=cvs_returned', alert: true }
	]);
</script>

<svelte:head><title>儀表板</title></svelte:head>

<PageHeader title="儀表板" />

<div class="mt-5 grid grid-cols-2 gap-3 md:grid-cols-5">
	{#each tiles as t (t.label)}
		<a href={t.href} class="card block transition-colors duration-150 hover:border-brand">
			<div class="text-sm text-ink-soft">{t.label}</div>
			<div class="mt-1 text-[28px] font-extrabold tabular-nums {t.alert && t.value !== '0' ? 'text-danger' : ''}">{t.value}</div>
			<div class="text-xs text-ink-soft">{t.note}</div>
		</a>
	{/each}
</div>

{#snippet list(title: string, items: AdminOrderListItem[], href: string)}
	<Card {title}>
		{#snippet actions()}<a {href} class="link text-sm">全部</a>{/snippet}
		{#if items.length === 0}
			<p class="text-sm text-ink-soft">沒有</p>
		{:else}
			<ul class="divide-y divide-line text-sm">
				{#each items as o (o.id)}
					<li class="flex items-center justify-between gap-2 py-2">
						<a href={`/admin/orders/${o.id}`} class="font-semibold hover:underline">{o.order_no}</a>
						<span class="min-w-0 flex-1 truncate text-ink-soft">{o.recipient_name}｜{o.item_count} 件｜{twd(o.total)}</span>
						<StatusBadge status={o.status} />
						<span class="text-xs text-ink-soft">{formatDate(o.created_at)}</span>
					</li>
				{/each}
			</ul>
		{/if}
	</Card>
{/snippet}

<div class="mt-6 grid gap-4 lg:grid-cols-2">
	{@render list('待出貨', d.pending_shipment_items, '/admin/orders?status=paid')}
	{@render list('需退款', d.needs_refund_items, '/admin/orders?flag=needs_refund')}
	{@render list('超商退回', d.cvs_returned_items, '/admin/orders?flag=cvs_returned')}
	{@render list('發票開立失敗', d.invoice_failed_items, '/admin/orders?flag=invoice_failed')}
</div>
