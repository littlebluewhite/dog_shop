<script lang="ts">
	import { cart } from '$lib/cart.svelte';
	import { twd } from '$lib/format';
</script>

<svelte:head><title>購物車</title></svelte:head>

<h1 class="text-2xl font-bold">購物車</h1>

{#if !cart.loaded}
	<p class="mt-4 text-gray-500">載入中…</p>
{:else if cart.lines.length === 0}
	<p class="mt-4 text-gray-600">購物車是空的。<a href="/products" class="underline">去逛逛</a></p>
{:else}
	<ul class="mt-4 divide-y divide-gray-200 rounded border border-gray-200 bg-white">
		{#each cart.lines as line (line.variant_id)}
			<li class="flex flex-wrap items-center gap-4 p-4">
				<a href={`/products/${line.product_slug}`} class="h-16 w-16 shrink-0 overflow-hidden rounded bg-gray-100">
					{#if line.image_thumb}<img src={line.image_thumb} alt="" class="h-full w-full object-cover" />{/if}
				</a>
				<div class="min-w-0 flex-1">
					<a href={`/products/${line.product_slug}`} class="font-medium hover:underline">{line.product_name}</a>
					<div class="text-sm text-gray-500">{line.variant_label}</div>
					<div class="text-sm">{twd(line.price)}</div>
				</div>
				<div class="flex items-center gap-1">
					<button type="button" class="h-8 w-8 rounded border border-gray-300" onclick={() => cart.setQty(line.variant_id, line.qty - 1)} aria-label="減少">−</button>
					<input
						type="number"
						min="1"
						max="99"
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
	<div class="mt-4 flex items-center justify-end gap-6">
		<div class="text-lg">小計 <span class="font-bold">{twd(cart.subtotal)}</span></div>
		<button type="button" disabled class="rounded bg-gray-400 px-6 py-2 text-white" title="結帳即將開放">前往結帳</button>
	</div>
	<p class="mt-2 text-right text-xs text-gray-500">價格與庫存會在結帳時重新確認。</p>
{/if}
