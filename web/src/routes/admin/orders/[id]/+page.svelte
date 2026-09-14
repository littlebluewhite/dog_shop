<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { type AdminAction, attentionNotes, availableActions } from '$lib/adminOrders';
	import { api, ApiError } from '$lib/api';
	import Alert from '$lib/components/ui/Alert.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import StatusBadge from '$lib/components/ui/StatusBadge.svelte';
	import { postToEcpay } from '$lib/ecpay';
	import { formatDate, twd } from '$lib/format';
	import { CVS_LABELS, INVOICE_LABELS, INVOICE_STATUS_LABELS, PAYMENT_LABELS, PAYMENT_STATUS_LABELS, SHIPMENT_STATUS_LABELS } from '$lib/labels';
	import { toast } from '$lib/toast.svelte';
	import type { EcpayForm } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
	const o = $derived(data.order);
	const actions = $derived(availableActions(o));
	const notes = $derived(attentionNotes(o));

	let busy = $state(false);
	let confirming = $state<AdminAction | null>(null);
	let carrier = $state('');
	let trackingNo = $state('');
	let errors = $state<Record<string, string>>({});

	function paymentLabel(method: string): string {
		return method === 'cod' ? '取貨付款' : PAYMENT_LABELS[method as keyof typeof PAYMENT_LABELS];
	}

	/** 打一個後台動作；成功後重新載入明細 */
	async function act(path: string, body: unknown = {}, done = '完成') {
		if (busy) return;
		busy = true;
		errors = {};
		try {
			await api(`/api/admin/orders/${o.id}/${path}`, { method: 'POST', body: JSON.stringify(body) });
			confirming = null;
			await invalidateAll();
			toast.show(done);
		} catch (err) {
			if (err instanceof ApiError) {
				errors = err.fields();
				const first = Object.values(errors)[0];
				toast.show(first ?? err.message, 5000);
			} else {
				toast.show('操作失敗，請再試一次');
			}
			confirming = null;
			await invalidateAll();
		} finally {
			busy = false;
		}
	}

	/** 列印託運單：拿簽好的表單，在新分頁 POST 到綠界（規格 §8.3） */
	async function printLabel() {
		if (busy) return;
		busy = true;
		try {
			const form = await api<EcpayForm>(`/api/admin/orders/${o.id}/print-label`, { method: 'POST', body: '{}' });
			postToEcpay(form, '_blank');
		} catch (err) {
			toast.show(err instanceof ApiError ? err.message : '無法取得託運單');
		} finally {
			busy = false;
		}
	}

	/** 兩段確認的動作：第一次按顯示確認鈕，第二次才真的送出 */
	const CONFIRM = {
		complete: { path: 'complete', label: '標記完成', confirm: '確定標記完成', done: '已標記完成', danger: false },
		cancel: { path: 'cancel', label: '取消訂單', confirm: '確定取消這筆訂單', done: '訂單已取消', danger: true },
		mark_refunded: { path: 'mark-refunded', label: '標記已退款', confirm: '確定已在綠界後台退款', done: '已標記退款', danger: true },
		clear_refund: { path: 'clear-refund', label: '已處理退款', confirm: '確定已處理退款', done: '已清除需退款', danger: true }
	} as const;
	type ConfirmAction = keyof typeof CONFIRM;
	const confirmActions = Object.keys(CONFIRM) as ConfirmAction[];
</script>

<svelte:head><title>訂單 {o.order_no}</title></svelte:head>

<a href="/admin/orders" class="link inline-flex items-center gap-1 text-sm"><Icon name="chevron-left" size={16} />訂單列表</a>
<div class="mt-2">
	<PageHeader
		title={`訂單 ${o.order_no}`}
		subtitle={`成立 ${formatDate(o.created_at)}${o.paid_at ? `｜付款 ${formatDate(o.paid_at)}` : ''}${o.shipped_at ? `｜出貨 ${formatDate(o.shipped_at)}` : ''}${o.completed_at ? `｜完成 ${formatDate(o.completed_at)}` : ''}${o.cancelled_at ? `｜結束 ${formatDate(o.cancelled_at)}` : ''}`}
	>
		<StatusBadge status={o.status} />
	</PageHeader>
</div>

{#if notes.length > 0}
	<Alert tone="danger" class="mt-4">
		<ul class="list-disc pl-5">
			{#each notes as n, i (i)}<li>{n}</li>{/each}
		</ul>
	</Alert>
{/if}

{#if actions.length > 0}
	<Card title="動作" class="mt-4">
		<div class="flex flex-wrap items-start gap-3">
			{#if actions.includes('ship_cvs')}
				<Button disabled={busy} onclick={() => act('ship-cvs', {}, '物流單已建立，訂單已出貨')}>
					建立物流單（{o.shipment?.cvs_sub_type ? CVS_LABELS[o.shipment.cvs_sub_type] : ''} {o.shipment?.cvs_store_name ?? ''}）
				</Button>
			{/if}
			{#if actions.includes('ship_home')}
				<form
					class="flex flex-wrap items-end gap-2"
					onsubmit={(e) => {
						e.preventDefault();
						void act('ship-home', { carrier, tracking_no: trackingNo }, '已出貨');
					}}
				>
					<Field label="貨運公司" type="text" bind:value={carrier} placeholder="例如 黑貓" error={errors.carrier} />
					<Field label="單號" type="text" bind:value={trackingNo} error={errors.tracking_no} />
					<Button type="submit" disabled={busy}>宅配出貨</Button>
				</form>
			{/if}
			{#if actions.includes('print_label')}
				<Button variant="secondary" disabled={busy} onclick={printLabel}>列印託運單</Button>
			{/if}
			{#if actions.includes('retry_invoice')}
				<Button variant="secondary" disabled={busy} onclick={() => act('retry-invoice', {}, '已重新排入開立')}>重開發票</Button>
			{/if}
			{#each confirmActions as a (a)}
				{#if actions.includes(a)}
					{@const c = CONFIRM[a]}
					{#if confirming === a}
						<span class="flex items-center gap-2">
							<Button variant={c.danger ? 'danger' : 'primary'} disabled={busy} onclick={() => act(c.path, {}, c.done)}>{c.confirm}</Button>
							<Button variant="secondary" onclick={() => (confirming = null)}>返回</Button>
						</span>
					{:else}
						<Button variant="secondary" disabled={busy} onclick={() => (confirming = a)}>{c.label}</Button>
					{/if}
				{/if}
			{/each}
		</div>
		{#if actions.includes('mark_refunded')}
			<p class="mt-2 text-xs text-ink-soft">退款要先在綠界廠商後台操作；這裡只記錄狀態{o.status === 'paid' ? '並把庫存加回去' : ''}。</p>
		{/if}
	</Card>
{/if}

<div class="card mt-4 overflow-x-auto p-0 md:p-0">
	<table class="table">
		<tbody>
			{#each o.items as item, i (i)}
				<tr>
					<td class="w-16">
						{#if item.image_path}<img src={item.image_path} alt="" class="h-12 w-12 rounded-lg object-cover" />{/if}
					</td>
					<td>{item.product_name}<div class="text-xs text-ink-soft">{item.variant_label}</div></td>
					<td class="text-right tabular-nums">{twd(item.unit_price)} × {item.quantity}</td>
					<td class="text-right tabular-nums">{twd(item.line_total)}</td>
				</tr>
			{/each}
		</tbody>
	</table>
	<div class="space-y-1 border-t border-line p-3 text-sm">
		<div class="flex justify-between"><span>商品小計</span><span class="tabular-nums">{twd(o.subtotal)}</span></div>
		<div class="flex justify-between"><span>運費</span><span class="tabular-nums">{o.shipping_fee === 0 ? '免運' : twd(o.shipping_fee)}</span></div>
		<div class="flex justify-between font-bold"><span>總計</span><span class="tabular-nums">{twd(o.total)}</span></div>
	</div>
</div>

<div class="mt-4 grid gap-4 md:grid-cols-2">
	<Card title="取貨">
		<div class="text-sm">
			<p>{o.recipient_name}　{o.recipient_phone}　<span class="text-ink-soft">{o.email}</span></p>
			{#if o.shipment}
				{@const s = o.shipment}
				{#if s.method === 'cvs'}
					<p class="mt-1">超商取貨：{s.cvs_sub_type ? CVS_LABELS[s.cvs_sub_type] : ''} {s.cvs_store_name}（{s.cvs_store_id}）</p>
					<p class="text-ink-soft">{s.cvs_store_address}</p>
				{:else}
					<p class="mt-1">宅配：{s.home_postal_code} {s.home_city}{s.home_district}{s.home_street}</p>
					{#if s.tracking_no}<p class="text-ink-soft">{s.carrier} {s.tracking_no}</p>{/if}
				{/if}
				<p class="mt-2">出貨狀態：<span class="font-semibold">{SHIPMENT_STATUS_LABELS[s.status]}</span></p>
				{#if s.ecpay_logistics_id}
					<p class="text-ink-soft">綠界物流單 {s.ecpay_logistics_id}｜廠商單號 {s.ecpay_merchant_trade_no}{#if s.cvs_payment_no}｜寄貨編號 {s.cvs_payment_no}{/if}{#if s.cvs_validation_no}｜驗證碼 {s.cvs_validation_no}{/if}</p>
				{/if}
				{#if s.last_status_code}
					<p class="text-ink-soft">最近通知：{s.last_status_code} {s.last_status_msg ?? ''}（{formatDate(s.updated_at)}）</p>
				{/if}
			{/if}
			{#if o.note}<p class="mt-2 text-ink-soft">買家備註：{o.note}</p>{/if}
		</div>
	</Card>
	<Card title="發票">
		<div class="text-sm">
			<p>{INVOICE_LABELS[o.invoice_type]}</p>
			{#if o.invoice_type === 'company'}
				<p class="text-ink-soft">統編 {o.invoice_tax_id}｜{o.invoice_title}｜{o.invoice_address}</p>
			{:else if o.invoice_type === 'donation'}
				<p class="text-ink-soft">愛心碼 {o.invoice_love_code}</p>
			{:else if o.invoice_carrier_num}
				<p class="text-ink-soft">載具 {o.invoice_carrier_num}</p>
			{/if}
			{#if o.invoice}
				<p class="mt-2">狀態：<span class="font-semibold">{INVOICE_STATUS_LABELS[o.invoice.status]}</span></p>
				{#if o.invoice.invoice_no}
					<p class="text-ink-soft">發票號碼 {o.invoice.invoice_no}　隨機碼 {o.invoice.random_number}{#if o.invoice.invoice_date}　{formatDate(o.invoice.invoice_date)}{/if}</p>
				{/if}
				{#if o.invoice.error}<p class="font-medium text-danger">{o.invoice.error}</p>{/if}
			{/if}
		</div>
	</Card>
</div>

<Card title="付款嘗試" class="mt-4">
	<div class="-mx-4 overflow-x-auto md:-mx-5">
		<table class="table">
			<thead>
				<tr>
					<th>綠界交易編號</th>
					<th>方式</th>
					<th>狀態</th>
					<th>金額</th>
					<th>綠界單號</th>
					<th>繳費資訊</th>
					<th>時間</th>
				</tr>
			</thead>
			<tbody>
				{#each o.payments as p (p.id)}
					<tr>
						<td class="tabular-nums">{p.merchant_trade_no}</td>
						<td>{paymentLabel(p.method)}</td>
						<td>{PAYMENT_STATUS_LABELS[p.status]}</td>
						<td class="tabular-nums">{twd(p.amount)}</td>
						<td class="tabular-nums">{p.ecpay_trade_no ?? '—'}</td>
						<td class="text-ink-soft">
							{#if p.atm_vaccount}銀行 {p.atm_bank_code} 帳號 {p.atm_vaccount}{:else if p.cvs_payment_no}代碼 {p.cvs_payment_no}{:else}—{/if}
							{#if p.expire_at}<div class="text-xs">期限 {formatDate(p.expire_at)}</div>{/if}
						</td>
						<td class="text-ink-soft">{formatDate(p.payment_date ?? p.created_at)}</td>
					</tr>
				{:else}
					<tr><td colspan="7" class="p-4 text-center text-ink-soft">沒有付款紀錄</td></tr>
				{/each}
			</tbody>
		</table>
	</div>
</Card>
