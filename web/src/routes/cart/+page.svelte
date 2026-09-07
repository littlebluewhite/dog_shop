<script lang="ts">
	import { api } from '$lib/api';
	import { cart } from '$lib/cart.svelte';
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
				if (item.qty !== cart.lines.find((l) => l.variant_id === item.variant_id)?.qty) {
					cart.setQty(item.variant_id, item.qty);
				}
			}
			checked = res;
		} catch {
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

	const problems = $derived(checked?.items.filter((i) => !i.available) ?? []);
	function problemOf(variant_id: string) {
		return checked?.items.find((i) => i.variant_id === variant_id);
	}
	function removeUnavailable() {
		for (const p of problems) cart.remove(p.variant_id);
		void validate();
	}
	const canCheckout = $derived(cart.lines.length > 0 && !checking && problems.length === 0);
</script>

<svelte:head><title>購物車</title></svelte:head>

<h1 class="text-2xl font-bold">購物車</h1>

{#if !cart.loaded}
	<p class="mt-4 text-gray-500">載入中…</p>
{:else if cart.lines.length === 0}
	<p class="mt-4 text-gray-600">購物車是空的。<a href="/products" class="underline">去逛逛</a></p>
{:else}
	{#if problems.length > 0}
		<div class="mt-4 flex flex-wrap items-center gap-3 rounded border border-yellow-300 bg-yellow-50 p-3 text-sm">
			<span>有 {problems.length} 項商品目前無法購買，請移除後再結帳。</span>
			<button type="button" onclick={removeUnavailable} class="rounded bg-gray-900 px-3 py-1 text-white">移除無法購買的商品</button>
		</div>
	{/if}
	<ul class="mt-4 divide-y divide-gray-200 rounded border border-gray-200 bg-white">
		{#each cart.lines as line (line.variant_id)}
			{@const p = problemOf(line.variant_id)}
			<li class="flex flex-wrap items-center gap-4 p-4">
				<a href={`/products/${line.product_slug}`} class="h-16 w-16 shrink-0 overflow-hidden rounded bg-gray-100">
					{#if line.image_thumb}<img src={line.image_thumb} alt="" class="h-full w-full object-cover" />{/if}
				</a>
				<div class="min-w-0 flex-1">
					<a href={`/products/${line.product_slug}`} class="font-medium hover:underline">{line.product_name}</a>
					<div class="text-sm text-gray-500">{line.variant_label}</div>
					<div class="text-sm">{twd(line.price)}</div>
					{#if p?.reason === 'unavailable'}
						<div class="text-sm text-red-600">已下架</div>
					{:else if p?.reason === 'sold_out'}
						<div class="text-sm text-red-600">已售完</div>
					{:else if p?.reason === 'qty_reduced'}
						<div class="text-sm text-yellow-700">庫存只剩 {p.stock} 件，數量已調整</div>
					{/if}
				</div>
				<div class="flex items-center gap-1">
					<button type="button" class="h-8 w-8 rounded border border-gray-300" onclick={() => cart.setQty(line.variant_id, line.qty - 1)} aria-label="減少">−</button>
					<input
						type="number"
						min="1"
						max={p?.available ? p.stock : 99}
						value={line.qty}
						onchange={(e) => cart.setQty(line.variant_id, Number(e.currentTarget.value))}
						class="w-14 rounded border border-gray-300 px-2 py-1 text-center"
						aria-label="數量"
					/>
					<button type="button" class="h-8 w-8 rounded border border-gray-300" onclick={() => cart.setQty(line.variant_id, line.qty + 1)} aria-label="增加">+</button>
				</div>
				<div class="w-24 text-right font-medium">{twd(line.price * line.qty)}</div>
				<button type="button" class="text-sm text-gray-500 hover:text-red-600" onclick={() => cart.remove(line.variant_id)}>移除</button>
			</li>
		{/each}
	</ul>
	<div class="mt-4 flex flex-wrap items-center justify-end gap-4">
		<button type="button" onclick={validate} disabled={checking} class="text-sm underline disabled:opacity-50">
			{checking ? '確認中…' : '重新確認庫存'}
		</button>
		<div class="text-lg">小計 <span class="font-bold">{twd(cart.subtotal)}</span></div>
		{#if canCheckout}
			<a href="/checkout" class="rounded bg-gray-900 px-6 py-2 text-white">前往結帳</a>
		{:else}
			<span class="rounded bg-gray-400 px-6 py-2 text-white">前往結帳</span>
		{/if}
	</div>
	{#if cart.subtotal > CVS_SUBTOTAL_LIMIT}
		<p class="mt-2 text-right text-sm text-yellow-700">商品小計超過 {twd(CVS_SUBTOTAL_LIMIT)}，只能選擇宅配。</p>
	{/if}
	<p class="mt-2 text-right text-xs text-gray-500">價格與庫存會在結帳時再次確認。</p>
{/if}
