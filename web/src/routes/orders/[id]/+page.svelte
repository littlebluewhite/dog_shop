<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import { formatDate, twd } from '$lib/format';
	import { CVS_LABELS, INVOICE_LABELS, ORDER_STATUS_LABELS, PAYMENT_LABELS } from '$lib/labels';
	import { toast } from '$lib/toast.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const o = $derived(data.order);
	let confirming = $state(false);
	let cancelling = $state(false);

	async function cancel() {
		if (cancelling) return;
		cancelling = true;
		try {
			const qs = data.token ? `?t=${encodeURIComponent(data.token)}` : '';
			await api(`/api/orders/${o.id}/cancel${qs}`, { method: 'POST' });
			confirming = false;
			await invalidateAll();
			toast.show('訂單已取消');
		} catch (err) {
			toast.show(err instanceof ApiError ? err.message : '取消失敗');
			confirming = false;
			await invalidateAll();
		} finally {
			cancelling = false;
		}
	}
</script>

<svelte:head><title>訂單 {o.order_no}</title></svelte:head>

<div class="flex flex-wrap items-baseline justify-between gap-2">
	<h1 class="text-2xl font-bold">訂單 {o.order_no}</h1>
	<span class="rounded bg-gray-900 px-3 py-1 text-sm text-white">{ORDER_STATUS_LABELS[o.status]}</span>
</div>
<p class="mt-1 text-sm text-gray-500">成立時間 {formatDate(o.created_at)}</p>

{#if o.status === 'pending_payment'}
	<div class="mt-4 rounded border border-yellow-300 bg-yellow-50 p-4 text-sm">
		<p class="font-medium">付款方式：{PAYMENT_LABELS[o.payment?.method ?? 'credit']}</p>
		<p class="mt-1 text-gray-700">線上付款功能準備中；訂單已為你保留商品。</p>
	</div>
{:else if o.status === 'cancelled'}
	<div class="mt-4 rounded border border-gray-300 bg-gray-50 p-4 text-sm">
		這筆訂單已取消{#if o.cancelled_at}（{formatDate(o.cancelled_at)}）{/if}。
	</div>
{/if}

{#if data.token && !data.user}
	<p class="mt-4 text-sm text-gray-600">請把這個網頁的網址存起來，之後用它查看訂單。</p>
{/if}

<section class="mt-6 rounded border border-gray-200 bg-white">
	<ul class="divide-y divide-gray-100">
		{#each o.items as item, i (i)}
			<li class="flex items-center gap-4 p-4">
				<div class="h-14 w-14 shrink-0 overflow-hidden rounded bg-gray-100">
					{#if item.image_path}<img src={item.image_path} alt="" class="h-full w-full object-cover" />{/if}
				</div>
				<div class="min-w-0 flex-1">
					<div class="font-medium">{item.product_name}</div>
					<div class="text-sm text-gray-500">{item.variant_label} × {item.quantity}</div>
				</div>
				<div class="text-right">
					<div>{twd(item.line_total)}</div>
					<div class="text-xs text-gray-500">單價 {twd(item.unit_price)}</div>
				</div>
			</li>
		{/each}
	</ul>
	<div class="space-y-1 border-t border-gray-200 p-4 text-sm">
		<div class="flex justify-between"><span>商品小計</span><span>{twd(o.subtotal)}</span></div>
		<div class="flex justify-between"><span>運費</span><span>{o.shipping_fee === 0 ? '免運' : twd(o.shipping_fee)}</span></div>
		<div class="flex justify-between text-base font-bold"><span>總計</span><span>{twd(o.total)}</span></div>
	</div>
</section>

<div class="mt-6 grid gap-4 md:grid-cols-2">
	<section class="rounded border border-gray-200 bg-white p-4 text-sm">
		<h2 class="font-medium">取貨</h2>
		<p class="mt-2">{o.recipient_name}　{o.recipient_phone}</p>
		{#if o.shipment?.method === 'cvs'}
			<p class="mt-1">超商取貨：{o.shipment.cvs_sub_type ? CVS_LABELS[o.shipment.cvs_sub_type] : ''} {o.shipment.cvs_store_name}</p>
			<p class="text-gray-600">{o.shipment.cvs_store_address}</p>
		{:else if o.shipment}
			<p class="mt-1">宅配：{o.shipment.home_postal_code} {o.shipment.home_city}{o.shipment.home_district}{o.shipment.home_street}</p>
			{#if o.shipment.tracking_no}<p class="text-gray-600">{o.shipment.carrier} {o.shipment.tracking_no}</p>{/if}
		{/if}
		{#if o.note}<p class="mt-2 text-gray-600">備註：{o.note}</p>{/if}
	</section>
	<section class="rounded border border-gray-200 bg-white p-4 text-sm">
		<h2 class="font-medium">發票</h2>
		<p class="mt-2">{INVOICE_LABELS[o.invoice_type]}</p>
		{#if o.invoice_type === 'company'}
			<p class="text-gray-600">統編 {o.invoice_tax_id}｜{o.invoice_title}</p>
			<p class="text-gray-600">{o.invoice_address}</p>
		{:else if o.invoice_type === 'donation'}
			<p class="text-gray-600">愛心碼 {o.invoice_love_code}</p>
		{:else if o.invoice_carrier_num}
			<p class="text-gray-600">載具 {o.invoice_carrier_num}</p>
		{:else}
			<p class="text-gray-600">發票會寄到 {o.email}</p>
		{/if}
	</section>
</div>

{#if o.status === 'pending_payment'}
	<div class="mt-6 flex items-center gap-3">
		{#if confirming}
			<button type="button" onclick={cancel} disabled={cancelling} class="rounded bg-red-600 px-4 py-2 text-white disabled:opacity-50">
				{cancelling ? '取消中…' : '確定取消這筆訂單'}
			</button>
			<button type="button" onclick={() => (confirming = false)} class="rounded border border-gray-300 px-4 py-2">保留</button>
		{:else}
			<button type="button" onclick={() => (confirming = true)} class="text-sm text-gray-600 underline">取消訂單</button>
		{/if}
	</div>
{/if}

{#if data.user}
	<p class="mt-8 text-sm"><a href="/account/orders" class="underline">回我的訂單</a></p>
{/if}
