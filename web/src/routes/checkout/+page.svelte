<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import { goto } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import { cart } from '$lib/cart.svelte';
	import { CHECKOUT_STORAGE_KEY, defaultForm, isMobileDevice, storeErrorMessage, toOrderInput, validateForm, type CheckoutForm } from '$lib/checkout';
	import AddressFields from '$lib/components/AddressFields.svelte';
	import Alert from '$lib/components/ui/Alert.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import InvoiceFields from '$lib/components/checkout/InvoiceFields.svelte';
	import { postToEcpay } from '$lib/ecpay';
	import { twd } from '$lib/format';
	import { CVS_LABELS, PAYMENT_LABELS } from '$lib/labels';
	import { toast } from '$lib/toast.svelte';
	import type { CartValidateResponse, CvsStore, CvsSubType, EcpayForm, OrderCreated, PaymentMethod } from '$lib/types';
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
	let pickingStore = $state(false);

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
		const storeError = storeErrorMessage(data.storeError);
		if (storeError) {
			form.shipping_method = 'cvs';
			toast.show(storeError, 5000);
		}
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

	/** 去綠界電子地圖選門市（規格 §7 第 2 點）：草稿已由上面的 $effect 存進 sessionStorage；頂層導頁離開本頁 */
	async function pickStore() {
		if (pickingStore) return;
		pickingStore = true;
		try {
			const mapForm = await api<EcpayForm>('/api/checkout/cvs-map', {
				method: 'POST',
				body: JSON.stringify({ sub_type: form.cvs_sub_type, device: isMobileDevice(navigator.userAgent) ? 1 : 0 })
			});
			postToEcpay(mapForm);
		} catch (err) {
			toast.show(err instanceof ApiError ? err.message : '無法開啟門市地圖，請再試一次');
			pickingStore = false;
		}
	}

	/** 換了超商就把上次選的門市清掉（門市屬於某一家超商） */
	function onSubTypeChange() {
		if (store && store.sub_type !== form.cvs_sub_type) store = null;
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
		let ecpay: EcpayForm | null = null;
		let created: OrderCreated | null = null;
		try {
			const body = toOrderInput(
				form,
				lines.map((l) => ({ variant_id: l.variant_id, qty: l.qty })),
				store?.token ?? null
			);
			created = await api<OrderCreated>('/api/orders', { method: 'POST', body: JSON.stringify(body) });
			cart.clear();
			try {
				sessionStorage.removeItem(CHECKOUT_STORAGE_KEY);
			} catch {
				// ignore
			}
			ecpay = created.ecpay;
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
		if (ecpay) {
			// 訂單已成立、購物車已清空；離開本頁去綠界付款（規格 §7 第 7 點）。按鈕維持停用，避免重複送出
			submitting = true;
			postToEcpay(ecpay);
		} else if (created !== null) {
			// 組綠界表單失敗（罕見）：訂單已成立，先帶去訂單頁；與後端 ClientBackURL 同一套規則（會員不用 token、訪客帶 ?t=，api/src/routes/orders.rs:109）
			await goto(data.user ? `/orders/${created.order_id}` : `/orders/${created.order_id}?t=${created.guest_token}`);
		}
	}
</script>

<PageHeader title="結帳" />

{#if !cart.loaded}
	<p class="mt-4 text-ink-soft">載入中…</p>
{:else if cart.lines.length === 0}
	<div class="mt-4">
		<EmptyState message="購物車是空的。"><Button variant="secondary" href="/products">去逛逛</Button></EmptyState>
	</div>
{:else}
	<form onsubmit={submit} class="mt-6 grid gap-8 lg:grid-cols-[1fr_20rem]" novalidate>
		<div class="space-y-6">
			<!-- 1 聯絡與收件人 -->
			<Card title="聯絡資料" step={1}>
				<div class="space-y-3">
					{#if !data.user}
						<p class="text-sm text-ink-soft">
							訪客結帳；<a href="/login?redirect=/checkout" class="link">登入</a>可以用常用地址、在會員中心看訂單。
						</p>
					{/if}
					<Field label="Email（訂單通知寄到這裡）" type="email" bind:value={form.email} autocomplete="email" error={errors.email} />
					{#if data.addresses.length > 0}
						<Field label="常用地址">
							<div class="mt-1 flex gap-2">
								<select bind:value={selectedAddressId} class="input">
									<option value="">選一個帶入</option>
									{#each data.addresses as a (a.id)}
										<option value={a.id}>{a.recipient_name}｜{a.city}{a.district}{a.street}</option>
									{/each}
								</select>
								<Button variant="secondary" onclick={useAddress}>帶入</Button>
							</div>
						</Field>
					{/if}
					<div class="grid grid-cols-2 gap-3">
						<Field label="收件人" type="text" bind:value={form.recipient_name} autocomplete="name" error={errors.recipient_name} />
						<Field label="手機" type="tel" bind:value={form.recipient_phone} autocomplete="tel" placeholder="09xxxxxxxx" error={errors.recipient_phone} />
					</div>
				</div>
			</Card>
			<!-- 2 取貨方式 -->
			<Card title="取貨方式" step={2}>
				<div class="space-y-3">
					<div class="grid gap-2 sm:grid-cols-2">
						<label class="option-card"><input type="radio" bind:group={form.shipping_method} value="home" /> 宅配（{twd(shipping.home_fee)}）</label>
						<label class="option-card">
							<input type="radio" bind:group={form.shipping_method} value="cvs" disabled={cvsBlocked} /> 超商取貨（{twd(shipping.cvs_fee)}）
						</label>
					</div>
					{#if shipping.free_threshold > 0}
						<p class="text-sm text-ink-soft">商品小計滿 {twd(shipping.free_threshold)} 免運。</p>
					{/if}
					{#if cvsBlocked}<p class="text-sm text-warning">商品小計超過 {twd(CVS_SUBTOTAL_LIMIT)}，只能宅配。</p>{/if}
					{#if errors.shipping_method}<p class="field-error">{errors.shipping_method}</p>{/if}
					{#if form.shipping_method === 'home'}
						<AddressFields bind:address={form.address} {errors} prefix="address." />
					{:else}
						<div class="grid gap-2 sm:grid-cols-3">
							{#each cvsTypes as t (t)}
								<label class="option-card"><input type="radio" bind:group={form.cvs_sub_type} value={t} onchange={onSubTypeChange} /> {CVS_LABELS[t]}</label>
							{/each}
						</div>
						{#if store}
							<Alert tone="info">
								<p class="font-semibold">{CVS_LABELS[store.sub_type]} {store.store_name}（{store.store_id}）</p>
								<p class="text-ink-soft">{store.store_address}</p>
							</Alert>
						{/if}
						<Button variant="secondary" onclick={pickStore} disabled={pickingStore}>
							{pickingStore ? '前往綠界地圖…' : store ? '重新選擇門市' : '選擇門市'}
						</Button>
						{#if errors.cvs_store}<p class="field-error">{errors.cvs_store}</p>{/if}
						<p class="text-xs text-ink-soft">超商取貨收件人請填 2～5 個中文字的本名，取貨時要核對證件。</p>
					{/if}
				</div>
			</Card>
			<!-- 3 發票 -->
			<Card title="發票" step={3}>
				<InvoiceFields bind:invoice={form.invoice} {errors} />
			</Card>
			<!-- 4 付款方式 -->
			<Card title="付款方式" step={4}>
				<div class="space-y-3">
					<div class="grid gap-2 sm:grid-cols-3">
						{#each enabledPayments as m (m)}
							<label class="option-card"><input type="radio" bind:group={form.payment_method} value={m} /> {PAYMENT_LABELS[m]}</label>
						{/each}
					</div>
					{#if errors.payment_method}<p class="field-error">{errors.payment_method}</p>{/if}
					<Field label="備註（選填，最多 200 字）" error={errors.note}>
						{#snippet children({ errorId, invalid })}
							<textarea bind:value={form.note} rows="2" class="input mt-1" aria-invalid={invalid} aria-describedby={errorId}></textarea>
						{/snippet}
					</Field>
				</div>
			</Card>
		</div>
		<!-- 摘要（規格 §5.7）：桌機黏右欄；手機在最下面、不黏 -->
		<aside class="h-fit lg:sticky lg:top-20">
			<Card title="訂單摘要">
				<div class="space-y-3 text-sm">
					{#if checking}<p class="text-ink-soft">確認庫存中…</p>{/if}
					<ul class="divide-y divide-line">
						{#each lines as l (l.variant_id)}
							<li class="flex justify-between gap-2 py-2">
								<span class="min-w-0 truncate">{l.product_name}<span class="text-ink-soft">（{l.variant_label}）× {l.qty}</span></span>
								<span class="shrink-0 tabular-nums">{twd(l.price * l.qty)}</span>
							</li>
						{/each}
					</ul>
					{#if checked && checked.items.some((i) => !i.available)}
						<p class="text-danger">有商品無法購買，請回<a href="/cart" class="link">購物車</a>處理。</p>
					{/if}
					{#each reduced as r, i (i)}
						<p class="text-warning">{r.name} 庫存不足，數量已調整為 {r.qty}</p>
					{/each}
					{#if checkFailed}
						<p class="text-danger">無法確認庫存，請重新確認</p>
						<Button variant="secondary" class="w-full" onclick={validateCart} disabled={checking}>重新確認庫存</Button>
					{/if}
					{#if checked}
						<div class="flex justify-between"><span>商品小計</span><span class="tabular-nums">{twd(subtotal)}</span></div>
						<div class="flex justify-between"><span>運費</span><span class="tabular-nums">{fee === 0 ? '免運' : twd(fee)}</span></div>
						<div class="flex justify-between text-lg font-bold"><span>總計</span><span class="tabular-nums">{twd(total)}</span></div>
					{/if}
					<Button type="submit" size="lg" class="w-full" disabled={submitting || checking || lines.length === 0}>
						{submitting ? '送出中…' : '送出訂單'}
					</Button>
					<p class="text-xs text-ink-soft">送出後會建立訂單並前往綠界付款頁；付款完成會回到訂單頁。</p>
				</div>
			</Card>
		</aside>
	</form>
{/if}
