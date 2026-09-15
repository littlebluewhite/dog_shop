<script lang="ts">
	import { api } from '$lib/api';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Alert from '$lib/components/ui/Alert.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import QtyStepper from '$lib/components/ui/QtyStepper.svelte';
	import { cart, cartQtyMax } from '$lib/cart.svelte';
	import { twd } from '$lib/format';
	import { toast } from '$lib/toast.svelte';
	import type { CartValidateResponse } from '$lib/types';
	import { CVS_SUBTOTAL_LIMIT } from '$lib/validation';

	let checked = $state<CartValidateResponse | null>(null);
	let checking = $state(false);
	let checkedOnce = false;

	/** 向伺服器核對價格與庫存（規格 §6.1）；把結果同步回 localStorage 的購物車 */
	async function validate() {
		if (cart.lines.length === 0) {
			checked = null;
			return;
		}
		checking = true;
		try {
			const res = await api<CartValidateResponse>('/api/cart/validate', {
				method: 'POST',
				body: JSON.stringify({ items: cart.lines.map((l) => ({ variant_id: l.variant_id, qty: l.qty })) })
			});
			for (const item of res.items) {
				if (!item.available) continue;
				cart.update(item.variant_id, {
					price: item.price,
					product_name: item.product_name,
					variant_label: item.variant_label,
					image_thumb: item.image_thumb,
					product_slug: item.product_slug
				});
				const local = cart.lines.find((l) => l.variant_id === item.variant_id)?.qty;
				if (local !== undefined && local > item.stock) cart.setQty(item.variant_id, item.stock);
			}
			checked = res;
		} catch {
			checked = null;
			toast.show('無法確認庫存，請稍後再試');
		} finally {
			checking = false;
		}
	}

	// 購物車從 localStorage 載入後核對一次
	$effect(() => {
		if (cart.loaded && !checkedOnce) {
			checkedOnce = true;
			void validate();
		}
	});

	const problems = $derived((checked?.items ?? []).filter((i) => !i.available && cart.lines.some((l) => l.variant_id === i.variant_id)));
	function problemOf(variant_id: string) {
		return checked?.items.find((i) => i.variant_id === variant_id);
	}
	function removeUnavailable() {
		for (const p of problems) cart.remove(p.variant_id);
		void validate();
	}
	const canCheckout = $derived(cart.lines.length > 0 && !checking && checked !== null && problems.length === 0);
</script>

<svelte:head><title>購物車</title></svelte:head>

<PageHeader title="購物車" />

{#if !cart.loaded}
	<p class="mt-4 text-ink-soft">載入中…</p>
{:else if cart.lines.length === 0}
	<div class="mt-4">
		<EmptyState message="購物車是空的。"><Button variant="secondary" href="/products">去逛逛</Button></EmptyState>
	</div>
{:else}
	{#if problems.length > 0}
		<Alert tone="warning" class="mt-4">
			<div class="flex flex-wrap items-center gap-3">
				<span>有 {problems.length} 項商品目前無法購買，請移除後再結帳。</span>
				<Button size="sm" onclick={removeUnavailable}>移除無法購買的商品</Button>
			</div>
		</Alert>
	{/if}
	<div class="mt-4 flex flex-col gap-6 lg:grid lg:grid-cols-[1fr_20rem]">
		<ul class="space-y-3">
			{#each cart.lines as line (line.variant_id)}
				{@const p = problemOf(line.variant_id)}
				<li>
					<Card>
						<div class="flex flex-wrap items-center gap-4">
							<a href={`/products/${line.product_slug}`} class="h-20 w-20 shrink-0 overflow-hidden rounded-control bg-surface">
								{#if line.image_thumb}<img src={line.image_thumb} alt="" class="h-full w-full object-cover" />{/if}
							</a>
							<div class="min-w-0 flex-1">
								<a href={`/products/${line.product_slug}`} class="font-semibold hover:underline">{line.product_name}</a>
								<div class="text-sm text-ink-soft">{line.variant_label}</div>
								<div class="text-sm tabular-nums">{twd(line.price)}</div>
								{#if p?.reason === 'unavailable'}
									<div class="text-sm font-medium text-danger">已下架</div>
								{:else if p?.reason === 'sold_out'}
									<div class="text-sm font-medium text-danger">已售完</div>
								{:else if p?.reason === 'qty_reduced'}
									<div class="text-sm font-medium text-warning">庫存只剩 {p.stock} 件，數量已調整</div>
								{/if}
							</div>
							<div class="flex basis-full items-center justify-between gap-3 md:basis-auto md:gap-4">
								<QtyStepper value={line.qty} min={0} max={cartQtyMax(p?.available ? p.stock : undefined)} onchange={(v) => cart.setQty(line.variant_id, v)} />
								<div class="w-24 text-right font-bold tabular-nums">{twd(line.price * line.qty)}</div>
								<Button variant="ghost" size="sm" onclick={() => cart.remove(line.variant_id)}>移除</Button>
							</div>
						</div>
					</Card>
				</li>
			{/each}
		</ul>
		<!-- 摘要：桌機黏在右欄上方；手機在清單下面、黏在螢幕底部（規格 §5.6） -->
		<aside class="sticky bottom-0 z-30 h-fit lg:top-20 lg:bottom-auto">
			<Card class="shadow-[0_-8px_24px_rgba(22,39,74,.08)] lg:shadow-none">
				<div class="flex items-baseline justify-between">
					<span class="text-lg">小計</span>
					<span class="text-[22px] font-bold tabular-nums">{twd(cart.subtotal)}</span>
				</div>
				<div class="mt-3">
					<Button href="/checkout" size="lg" class="w-full" disabled={!canCheckout}>前往結帳</Button>
				</div>
				<div class="mt-3 flex justify-center">
					<Button variant="ghost" size="sm" onclick={validate} disabled={checking}>{checking ? '確認中…' : '重新確認庫存'}</Button>
				</div>
				{#if cart.subtotal > CVS_SUBTOTAL_LIMIT}
					<p class="mt-2 text-sm text-warning">商品小計超過 {twd(CVS_SUBTOTAL_LIMIT)}，只能選擇宅配。</p>
				{/if}
				<p class="mt-2 text-xs text-ink-soft">價格與庫存會在結帳時再次確認。</p>
			</Card>
		</aside>
	</div>
{/if}
