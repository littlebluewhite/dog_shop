<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import { cart } from '$lib/cart.svelte';
	import { CHECKOUT_STORAGE_KEY, defaultForm, toOrderInput, validateForm, type CheckoutForm } from '$lib/checkout';
	import AddressFields from '$lib/components/AddressFields.svelte';
	import InvoiceFields from '$lib/components/checkout/InvoiceFields.svelte';
	import { twd } from '$lib/format';
	import { CVS_LABELS, PAYMENT_LABELS } from '$lib/labels';
	import { toast } from '$lib/toast.svelte';
	import type { CartValidateResponse, CvsStore, CvsSubType, OrderCreated, PaymentMethod } from '$lib/types';
	import { CVS_SUBTOTAL_LIMIT, shippingFee } from '$lib/validation';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const shipping = untrack(() => data.settings.shipping);
	const enabledPayments = (['credit', 'atm', 'cvs_code'] as PaymentMethod[]).filter((m) => data.settings.payment_methods[m]);
	const cvsTypes: CvsSubType[] = ['UNIMARTC2C', 'FAMIC2C', 'HILIFEC2C'];

	// 初始值刻意只取一次（SSR 與 hydration 一致）；sessionStorage 的草稿在 onMount 才還原
	let form = $state<CheckoutForm>(untrack(() => defaultForm(data.user, enabledPayments[0] ?? 'credit')));
	let store = $state<CvsStore | null>(untrack(() => data.store));
	let checked = $state<CartValidateResponse | null>(null);
	let checking = $state(false);
	let checkFailed = $state(false);
	let reduced = $state<{ name: string; qty: number }[]>([]);
	let errors = $state<Record<string, string>>({});
	let submitting = $state(false);
	let selectedAddressId = $state('');

	const lines = $derived(checked?.items.filter((i) => i.available) ?? []);
	const subtotal = $derived(checked?.subtotal ?? cart.subtotal);
	const cvsBlocked = $derived(subtotal > CVS_SUBTOTAL_LIMIT);
	const fee = $derived(shippingFee(shipping, form.shipping_method, subtotal));
	const total = $derived(subtotal + fee);

	async function validateCart() {
		checkFailed = false;
		reduced = [];
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
				if (item.available && item.qty !== cart.lines.find((l) => l.variant_id === item.variant_id)?.qty) {
					cart.setQty(item.variant_id, item.qty);
				}
				if (item.reason === 'qty_reduced') {
					reduced.push({ name: item.product_name, qty: item.qty });
				}
			}
			checked = res;
		} catch {
			checkFailed = true;
			toast.show('無法確認庫存，請稍後再試');
		} finally {
			checking = false;
		}
	}

	function restoreDraft() {
		try {
			const raw = sessionStorage.getItem(CHECKOUT_STORAGE_KEY);
			if (!raw) return;
			const saved = JSON.parse(raw) as Partial<CheckoutForm>;
			form = { ...form, ...saved, address: { ...form.address, ...(saved.address ?? {}) }, invoice: { ...form.invoice, ...(saved.invoice ?? {}) } };
		} catch {
			// 壞掉的草稿就不管
		}
	}

	onMount(() => {
		restoreDraft();
		if (store) {
			form.shipping_method = 'cvs';
			form.cvs_sub_type = store.sub_type;
		}
		if (!enabledPayments.includes(form.payment_method)) form.payment_method = enabledPayments[0] ?? 'credit';
	});

	// 購物車載入後核對一次；表單每次變動就存草稿（計畫 4 去綠界地圖前後要用）
	let checkedOnce = false;
	$effect(() => {
		if (cart.loaded && !checkedOnce) {
			checkedOnce = true;
			void validateCart();
		}
	});
	$effect(() => {
		const snapshot = JSON.stringify(form);
		try {
			sessionStorage.setItem(CHECKOUT_STORAGE_KEY, snapshot);
		} catch {
			// 無痕模式：不存
		}
	});

	function useAddress() {
		const a = data.addresses.find((x) => x.id === selectedAddressId);
		if (!a) return;
		form.recipient_name = a.recipient_name;
		form.recipient_phone = a.phone;
		form.address = { postal_code: a.postal_code, city: a.city, district: a.district, street: a.street };
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		errors = validateForm(form, { subtotal, hasStore: store !== null, enabledPayments });
		if (Object.keys(errors).length > 0) {
			toast.show('請檢查紅字欄位');
			return;
		}
		if (lines.length === 0) {
			toast.show('購物車沒有可購買的商品');
			return;
		}
		submitting = true;
		let target: string | null = null;
		try {
			const body = toOrderInput(
				form,
				lines.map((l) => ({ variant_id: l.variant_id, qty: l.qty })),
				store?.token ?? null
			);
			const created = await api<OrderCreated>('/api/orders', { method: 'POST', body: JSON.stringify(body) });
			cart.clear();
			try {
				sessionStorage.removeItem(CHECKOUT_STORAGE_KEY);
			} catch {
				// ignore
			}
			target = data.user ? `/orders/${created.order_id}` : `/orders/${created.order_id}?t=${created.guest_token}`;
		} catch (err) {
			if (err instanceof ApiError && err.code === 'VALIDATION') {
				errors = err.fields();
				const first = Object.values(errors)[0];
				toast.show(first ? '請檢查紅字欄位：' + first : err.message);
			} else if (err instanceof ApiError && err.code === 'OUT_OF_STOCK') {
				toast.show('部分商品庫存不足，已重新確認，請檢查數量');
				await validateCart();
			} else if (err instanceof ApiError) {
				toast.show(err.message);
			} else {
				toast.show('送出失敗，請再試一次');
			}
		} finally {
			submitting = false;
		}
		if (target) await goto(target);
	}

	const input = 'mt-1 w-full rounded border border-gray-300 px-3 py-2';
</script>

<h1 class="text-2xl font-bold">結帳</h1>

{#if !cart.loaded}
	<p class="mt-4 text-gray-500">載入中…</p>
{:else if cart.lines.length === 0}
	<p class="mt-4 text-gray-600">購物車是空的。<a href="/products" class="underline">去逛逛</a></p>
{:else}
	<form onsubmit={submit} class="mt-6 grid gap-8 lg:grid-cols-[1fr_20rem]" novalidate>
		<div class="space-y-8">
			<!-- 聯絡與收件人 -->
			<section class="space-y-3 rounded border border-gray-200 bg-white p-4">
				<h2 class="font-medium">聯絡資料</h2>
				{#if !data.user}
					<p class="text-sm text-gray-600">
						訪客結帳；<a href="/login?redirect=/checkout" class="underline">登入</a>可以用常用地址、在會員中心看訂單。
					</p>
				{/if}
				<label class="block text-sm text-gray-700">
					Email（訂單通知寄到這裡）
					<input type="email" bind:value={form.email} autocomplete="email" class={input} />
					{#if errors.email}<span class="text-red-600">{errors.email}</span>{/if}
				</label>
				{#if data.addresses.length > 0}
					<label class="block text-sm text-gray-700">
						常用地址
						<div class="mt-1 flex gap-2">
							<select bind:value={selectedAddressId} class="w-full rounded border border-gray-300 px-2 py-2">
								<option value="">選一個帶入</option>
								{#each data.addresses as a (a.id)}
									<option value={a.id}>{a.recipient_name}｜{a.city}{a.district}{a.street}</option>
								{/each}
							</select>
							<button type="button" onclick={useAddress} class="shrink-0 rounded border border-gray-300 px-3">帶入</button>
						</div>
					</label>
				{/if}
				<div class="grid grid-cols-2 gap-3">
					<label class="block text-sm text-gray-700">
						收件人
						<input type="text" bind:value={form.recipient_name} autocomplete="name" class={input} />
						{#if errors.recipient_name}<span class="text-red-600">{errors.recipient_name}</span>{/if}
					</label>
					<label class="block text-sm text-gray-700">
						手機
						<input type="tel" bind:value={form.recipient_phone} autocomplete="tel" placeholder="09xxxxxxxx" class={input} />
						{#if errors.recipient_phone}<span class="text-red-600">{errors.recipient_phone}</span>{/if}
					</label>
				</div>
			</section>

			<!-- 取貨方式 -->
			<section class="space-y-3 rounded border border-gray-200 bg-white p-4">
				<h2 class="font-medium">取貨方式</h2>
				<div class="flex flex-wrap gap-4 text-sm">
					<label class="flex items-center gap-2"><input type="radio" bind:group={form.shipping_method} value="home" /> 宅配（{twd(shipping.home_fee)}）</label>
					<label class="flex items-center gap-2">
						<input type="radio" bind:group={form.shipping_method} value="cvs" disabled={cvsBlocked} /> 超商取貨（{twd(shipping.cvs_fee)}）
					</label>
				</div>
				{#if shipping.free_threshold > 0}
					<p class="text-sm text-gray-500">商品小計滿 {twd(shipping.free_threshold)} 免運。</p>
				{/if}
				{#if cvsBlocked}<p class="text-sm text-yellow-700">商品小計超過 {twd(CVS_SUBTOTAL_LIMIT)}，只能宅配。</p>{/if}
				{#if errors.shipping_method}<p class="text-sm text-red-600">{errors.shipping_method}</p>{/if}

				{#if form.shipping_method === 'home'}
					<AddressFields bind:address={form.address} {errors} prefix="address." />
				{:else}
					<div class="flex flex-wrap gap-4 text-sm">
						{#each cvsTypes as t (t)}
							<label class="flex items-center gap-2"><input type="radio" bind:group={form.cvs_sub_type} value={t} /> {CVS_LABELS[t]}</label>
						{/each}
					</div>
					{#if store}
						<div class="rounded bg-gray-50 p-3 text-sm">
							<div class="font-medium">{CVS_LABELS[store.sub_type]} {store.store_name}（{store.store_id}）</div>
							<div class="text-gray-600">{store.store_address}</div>
						</div>
					{/if}
					<button type="button" disabled class="rounded border border-gray-300 px-3 py-2 text-sm disabled:opacity-50" title="綠界電子地圖在下一個階段接上">
						選擇門市（門市選擇功能準備中）
					</button>
					{#if errors.cvs_store}<p class="text-sm text-red-600">{errors.cvs_store}</p>{/if}
					<p class="text-xs text-gray-500">超商取貨收件人請填 2～5 個中文字的本名，取貨時要核對證件。</p>
				{/if}
			</section>

			<!-- 發票 -->
			<section class="rounded border border-gray-200 bg-white p-4">
				<InvoiceFields bind:invoice={form.invoice} {errors} />
			</section>

			<!-- 付款方式 -->
			<section class="space-y-3 rounded border border-gray-200 bg-white p-4">
				<h2 class="font-medium">付款方式</h2>
				<div class="flex flex-wrap gap-4 text-sm">
					{#each enabledPayments as m (m)}
						<label class="flex items-center gap-2"><input type="radio" bind:group={form.payment_method} value={m} /> {PAYMENT_LABELS[m]}</label>
					{/each}
				</div>
				{#if errors.payment_method}<p class="text-sm text-red-600">{errors.payment_method}</p>{/if}
				<label class="block text-sm text-gray-700">
					備註（選填，最多 200 字）
					<textarea bind:value={form.note} rows="2" class={input}></textarea>
					{#if errors.note}<span class="text-red-600">{errors.note}</span>{/if}
				</label>
			</section>
		</div>

		<!-- 摘要 -->
		<aside class="h-fit space-y-3 rounded border border-gray-200 bg-white p-4 text-sm lg:sticky lg:top-4">
			<h2 class="font-medium">訂單摘要</h2>
			{#if checking}<p class="text-gray-500">確認庫存中…</p>{/if}
			<ul class="divide-y divide-gray-100">
				{#each lines as l (l.variant_id)}
					<li class="flex justify-between gap-2 py-2">
						<span class="min-w-0 truncate">{l.product_name}<span class="text-gray-500">（{l.variant_label}）× {l.qty}</span></span>
						<span class="shrink-0">{twd(l.price * l.qty)}</span>
					</li>
				{/each}
			</ul>
			{#if checked && checked.items.some((i) => !i.available)}
				<p class="text-red-600">有商品無法購買，請回<a href="/cart" class="underline">購物車</a>處理。</p>
			{/if}
			{#each reduced as r, i (i)}
				<p class="text-yellow-700">{r.name} 庫存不足，數量已調整為 {r.qty}</p>
			{/each}
			{#if checkFailed}
				<p class="text-red-600">無法確認庫存，請重新確認</p>
				<button type="button" onclick={validateCart} disabled={checking} class="w-full rounded border border-gray-300 px-3 py-2 disabled:opacity-50">
					重新確認庫存
				</button>
			{/if}
			{#if checked}
				<div class="flex justify-between"><span>商品小計</span><span>{twd(subtotal)}</span></div>
				<div class="flex justify-between"><span>運費</span><span>{fee === 0 ? '免運' : twd(fee)}</span></div>
				<div class="flex justify-between text-base font-bold"><span>總計</span><span>{twd(total)}</span></div>
			{/if}
			<button type="submit" disabled={submitting || checking || lines.length === 0} class="w-full rounded bg-gray-900 px-4 py-2 text-white disabled:opacity-50">
				{submitting ? '送出中…' : '送出訂單'}
			</button>
			<p class="text-xs text-gray-500">送出後會建立訂單並帶你到訂單頁；線上付款功能在下一個階段開放。</p>
		</aside>
	</form>
{/if}
