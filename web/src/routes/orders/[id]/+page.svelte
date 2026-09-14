<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import Alert from '$lib/components/ui/Alert.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import StatusBadge from '$lib/components/ui/StatusBadge.svelte';
	import { postToEcpay } from '$lib/ecpay';
	import { formatDate, twd } from '$lib/format';
	import { CVS_LABELS, INVOICE_LABELS, ORDER_STATUS_LABELS, PAYMENT_LABELS } from '$lib/labels';
	import { POLL_INTERVAL_MS, POLL_MAX_MS, hasPaymentInfo, needsPolling, paymentExpired } from '$lib/orderPage';
	import { toast } from '$lib/toast.svelte';
	import type { PaymentMethod, RepayResponse } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const o = $derived(data.order);
	const qs = $derived(data.token ? `?t=${encodeURIComponent(data.token)}` : '');
	const enabledPayments = $derived(
		(['credit', 'atm', 'cvs_code'] as PaymentMethod[]).filter((m) => data.settings.payment_methods[m])
	);
	let confirming = $state(false);
	let cancelling = $state(false);
	let repaying = $state(false);
	let pollTimedOut = $state(false);
	let repayMethod = $state<PaymentMethod>(untrack(() => data.order.payment?.method ?? 'credit'));

	// 付款中每 3 秒重新載入，最多 2 分鐘（規格 §6.1）；拿到繳費資訊或狀態改變就停
	onMount(() => {
		if (!needsPolling(untrack(() => o))) return;
		const started = Date.now();
		const id = setInterval(async () => {
			if (!needsPolling(o)) {
				clearInterval(id);
				return;
			}
			if (Date.now() - started >= POLL_MAX_MS) {
				clearInterval(id);
				pollTimedOut = true;
				return;
			}
			await invalidateAll();
		}, POLL_INTERVAL_MS);
		return () => clearInterval(id);
	});

	async function cancel() {
		if (cancelling) return;
		cancelling = true;
		try {
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

	/** 重新付款（規格 §7 第 9 點）：拿新表單後離開本頁去綠界 */
	async function repay() {
		if (repaying) return;
		repaying = true;
		try {
			const res = await api<RepayResponse>(`/api/orders/${o.id}/repay${qs}`, {
				method: 'POST',
				body: JSON.stringify({ payment_method: repayMethod })
			});
			postToEcpay(res.ecpay);
		} catch (err) {
			toast.show(err instanceof ApiError ? err.message : '重新付款失敗');
			repaying = false;
			await invalidateAll();
		}
	}
</script>

<svelte:head><title>訂單 {o.order_no}</title></svelte:head>

<PageHeader title={`訂單 ${o.order_no}`} subtitle={`成立時間 ${formatDate(o.created_at)}`}>
	<StatusBadge status={o.status} />
</PageHeader>

{#if o.status === 'pending_payment'}
	<Alert tone="warning" class="mt-5" title={`付款方式：${PAYMENT_LABELS[o.payment?.method ?? 'credit']}`}>
		{#if o.payment && hasPaymentInfo(o)}
			{#if o.payment.atm_vaccount}
				<p class="mt-2">請在期限內轉帳到下面的帳號：</p>
				<p class="mt-1 text-[22px] font-bold tabular-nums">銀行代碼 {o.payment.atm_bank_code}　帳號 {o.payment.atm_vaccount}</p>
			{:else}
				<p class="mt-2">請到超商多媒體機台輸入繳費代碼：</p>
				<p class="mt-1 text-[22px] font-bold tabular-nums">{o.payment.cvs_payment_no}</p>
			{/if}
			<p class="mt-1">
				金額 {twd(o.payment.amount)}{#if o.payment.expire_at}　繳費期限 {formatDate(o.payment.expire_at)}{/if}
			</p>
			{#if paymentExpired(o)}
				<p class="mt-2 font-medium text-danger">繳費期限已過，請重新付款。</p>
			{:else}
				<p class="mt-2">繳費後幾分鐘內會收到付款成功的 Email；重新整理這一頁也會更新。</p>
			{/if}
		{:else if pollTimedOut}
			<p class="mt-1">還沒收到付款結果。請重新整理這一頁；若已付款卻沒更新，請聯絡我們。</p>
		{:else}
			<p class="mt-1">等候綠界付款結果中…（每 3 秒自動更新）</p>
		{/if}
		<div class="mt-3 flex flex-wrap items-center gap-2">
			<label for="repay-method">重新付款：</label>
			<select id="repay-method" bind:value={repayMethod} class="input w-auto">
				{#each enabledPayments as m (m)}
					<option value={m}>{PAYMENT_LABELS[m]}</option>
				{/each}
			</select>
			<Button size="sm" onclick={repay} disabled={repaying}>{repaying ? '前往付款…' : '前往付款'}</Button>
		</div>
	</Alert>
{:else if o.status === 'paid' || o.status === 'shipped' || o.status === 'completed'}
	<Alert tone="success" class="mt-5">
		<p class="font-semibold">已付款{#if o.paid_at}（{formatDate(o.paid_at)}）{/if}，目前狀態：{ORDER_STATUS_LABELS[o.status]}</p>
	</Alert>
{:else if o.status === 'refunded'}
	<Alert tone="info" class="mt-5">這筆訂單已退款。</Alert>
{:else if o.status === 'cancelled'}
	<Alert tone="info" class="mt-5">這筆訂單已取消{#if o.cancelled_at}（{formatDate(o.cancelled_at)}）{/if}。</Alert>
{/if}

{#if data.token && !data.user}
	<p class="mt-4 text-sm text-ink-soft">請把這個網頁的網址存起來，之後用它查看訂單。</p>
{/if}

<Card class="mt-6 p-0 md:p-0">
	<ul class="divide-y divide-line">
		{#each o.items as item, i (i)}
			<li class="flex items-center gap-4 p-4">
				<div class="h-14 w-14 shrink-0 overflow-hidden rounded-lg bg-surface">
					{#if item.image_path}<img src={item.image_path} alt="" class="h-full w-full object-cover" />{/if}
				</div>
				<div class="min-w-0 flex-1">
					<div class="font-semibold">{item.product_name}</div>
					<div class="text-sm text-ink-soft">{item.variant_label} × {item.quantity}</div>
				</div>
				<div class="text-right">
					<div class="font-semibold tabular-nums">{twd(item.line_total)}</div>
					<div class="text-xs text-ink-soft">單價 {twd(item.unit_price)}</div>
				</div>
			</li>
		{/each}
	</ul>
	<div class="space-y-1 border-t border-line p-4 text-sm">
		<div class="flex justify-between"><span>商品小計</span><span class="tabular-nums">{twd(o.subtotal)}</span></div>
		<div class="flex justify-between"><span>運費</span><span class="tabular-nums">{o.shipping_fee === 0 ? '免運' : twd(o.shipping_fee)}</span></div>
		<div class="flex justify-between text-base font-bold"><span>總計</span><span class="tabular-nums">{twd(o.total)}</span></div>
	</div>
</Card>

<div class="mt-6 grid gap-4 md:grid-cols-2">
	<Card title="取貨">
		<div class="text-sm">
			<p>{o.recipient_name}　{o.recipient_phone}</p>
			{#if o.shipment?.method === 'cvs'}
				<p class="mt-1">超商取貨：{o.shipment.cvs_sub_type ? CVS_LABELS[o.shipment.cvs_sub_type] : ''} {o.shipment.cvs_store_name}</p>
				<p class="text-ink-soft">{o.shipment.cvs_store_address}</p>
			{:else if o.shipment}
				<p class="mt-1">宅配：{o.shipment.home_postal_code} {o.shipment.home_city}{o.shipment.home_district}{o.shipment.home_street}</p>
				{#if o.shipment.tracking_no}<p class="text-ink-soft">{o.shipment.carrier} {o.shipment.tracking_no}</p>{/if}
			{/if}
			{#if o.note}<p class="mt-2 text-ink-soft">備註：{o.note}</p>{/if}
		</div>
	</Card>
	<Card title="發票">
		<div class="text-sm">
			<p>{INVOICE_LABELS[o.invoice_type]}</p>
			{#if o.invoice_type === 'company'}
				<p class="text-ink-soft">統編 {o.invoice_tax_id}｜{o.invoice_title}</p>
				<p class="text-ink-soft">{o.invoice_address}</p>
			{:else if o.invoice_type === 'donation'}
				<p class="text-ink-soft">愛心碼 {o.invoice_love_code}</p>
			{:else if o.invoice_carrier_num}
				<p class="text-ink-soft">載具 {o.invoice_carrier_num}</p>
			{:else}
				<p class="text-ink-soft">發票會寄到 {o.email}</p>
			{/if}
			{#if o.invoice?.status === 'issued'}
				<p class="mt-2">
					發票號碼 {o.invoice.invoice_no}　隨機碼 {o.invoice.random_number}{#if o.invoice.invoice_date}　{formatDate(o.invoice.invoice_date)}{/if}
				</p>
			{:else if o.status === 'paid' || o.status === 'shipped' || o.status === 'completed'}
				<p class="mt-2 text-ink-soft">發票開立中，開好會通知您。</p>
			{/if}
		</div>
	</Card>
</div>

{#if o.status === 'pending_payment'}
	<div class="mt-6 flex items-center gap-3">
		{#if confirming}
			<Button variant="danger" onclick={cancel} disabled={cancelling}>{cancelling ? '取消中…' : '確定取消這筆訂單'}</Button>
			<Button variant="secondary" onclick={() => (confirming = false)}>保留</Button>
		{:else}
			<Button variant="ghost" size="sm" onclick={() => (confirming = true)}>取消訂單</Button>
		{/if}
	</div>
{/if}

{#if data.user}
	<p class="mt-8 text-sm"><a href="/account/orders" class="link">回我的訂單</a></p>
{/if}
