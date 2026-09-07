<script lang="ts">
	import { untrack } from 'svelte';
	import { invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import { CVS_LABELS } from '$lib/labels';
	import { toast } from '$lib/toast.svelte';
	import type { AllSettings, CvsSubType } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// 初始值刻意只取一次；存檔成功後用伺服器回傳的整組覆蓋
	let form = $state<AllSettings>(untrack(() => structuredClone(data.settings)));
	let errors = $state<Record<string, string>>({});
	let saving = $state(false);

	const cvsTypes: CvsSubType[] = ['UNIMARTC2C', 'FAMIC2C', 'HILIFEC2C'];
	const input = 'mt-1 w-full rounded border border-gray-300 px-3 py-2';

	async function save(e: SubmitEvent) {
		e.preventDefault();
		errors = {};
		saving = true;
		try {
			const body: AllSettings = {
				...form,
				shipping: {
					cvs_fee: Math.round(Number(form.shipping.cvs_fee) || 0),
					home_fee: Math.round(Number(form.shipping.home_fee) || 0),
					free_threshold: Math.round(Number(form.shipping.free_threshold) || 0)
				}
			};
			const saved = await api<AllSettings>('/api/admin/settings', { method: 'PUT', body: JSON.stringify(body) });
			form = saved;
			toast.show('已儲存');
			await invalidateAll();
		} catch (err) {
			if (err instanceof ApiError) {
				errors = err.fields();
				toast.show(Object.keys(errors).length > 0 ? '請檢查紅字欄位' : err.message);
			} else toast.show('儲存失敗');
		} finally {
			saving = false;
		}
	}
</script>

<svelte:head><title>商店設定</title></svelte:head>

<h1 class="text-2xl font-bold">商店設定</h1>

<form onsubmit={save} class="mt-6 max-w-2xl space-y-6" novalidate>
	<section class="space-y-3 rounded border border-gray-200 bg-white p-4">
		<h2 class="font-medium">商店資訊</h2>
		<label class="block text-sm text-gray-700">
			商店名稱
			<input type="text" bind:value={form.shop.name} class={input} />
			{#if errors['shop.name']}<span class="text-red-600">{errors['shop.name']}</span>{/if}
		</label>
		<label class="block text-sm text-gray-700">
			簡介（選填）
			<textarea bind:value={form.shop.description} rows="3" class={input}></textarea>
			{#if errors['shop.description']}<span class="text-red-600">{errors['shop.description']}</span>{/if}
		</label>
		<div class="grid grid-cols-2 gap-3">
			<label class="block text-sm text-gray-700">
				聯絡 Email
				<input type="email" bind:value={form.shop.contact_email} class={input} />
				{#if errors['shop.contact_email']}<span class="text-red-600">{errors['shop.contact_email']}</span>{/if}
			</label>
			<label class="block text-sm text-gray-700">
				聯絡電話
				<input type="text" bind:value={form.shop.contact_phone} class={input} />
				{#if errors['shop.contact_phone']}<span class="text-red-600">{errors['shop.contact_phone']}</span>{/if}
			</label>
		</div>
	</section>

	<section class="space-y-3 rounded border border-gray-200 bg-white p-4">
		<h2 class="font-medium">運費</h2>
		<div class="grid grid-cols-3 gap-3">
			<label class="block text-sm text-gray-700">
				超商取貨（元）
				<input type="number" min="0" bind:value={form.shipping.cvs_fee} class={input} />
				{#if errors['shipping.cvs_fee']}<span class="text-red-600">{errors['shipping.cvs_fee']}</span>{/if}
			</label>
			<label class="block text-sm text-gray-700">
				宅配（元）
				<input type="number" min="0" bind:value={form.shipping.home_fee} class={input} />
				{#if errors['shipping.home_fee']}<span class="text-red-600">{errors['shipping.home_fee']}</span>{/if}
			</label>
			<label class="block text-sm text-gray-700">
				免運門檻（元，0 = 不免運）
				<input type="number" min="0" bind:value={form.shipping.free_threshold} class={input} />
				{#if errors['shipping.free_threshold']}<span class="text-red-600">{errors['shipping.free_threshold']}</span>{/if}
			</label>
		</div>
		<p class="text-xs text-gray-500">免運以商品小計比較；超商取貨商品小計上限 20,000 元（綠界規定）。</p>
	</section>

	<section class="space-y-3 rounded border border-gray-200 bg-white p-4">
		<h2 class="font-medium">付款方式</h2>
		<div class="flex flex-wrap gap-4 text-sm">
			<label class="flex items-center gap-2"><input type="checkbox" bind:checked={form.payment_methods.credit} /> 信用卡</label>
			<label class="flex items-center gap-2"><input type="checkbox" bind:checked={form.payment_methods.atm} /> ATM 轉帳</label>
			<label class="flex items-center gap-2"><input type="checkbox" bind:checked={form.payment_methods.cvs_code} /> 超商代碼繳費</label>
		</div>
		{#if errors.payment_methods}<p class="text-sm text-red-600">{errors.payment_methods}</p>{/if}
	</section>

	<section class="space-y-3 rounded border border-gray-200 bg-white p-4">
		<h2 class="font-medium">寄件人（超商物流單用）</h2>
		<div class="grid grid-cols-2 gap-3">
			<label class="block text-sm text-gray-700">
				姓名
				<input type="text" bind:value={form.sender.name} class={input} />
				{#if errors['sender.name']}<span class="text-red-600">{errors['sender.name']}</span>{/if}
			</label>
			<label class="block text-sm text-gray-700">
				手機
				<input type="tel" bind:value={form.sender.phone} placeholder="09xxxxxxxx" class={input} />
				{#if errors['sender.phone']}<span class="text-red-600">{errors['sender.phone']}</span>{/if}
			</label>
		</div>
	</section>

	<section class="space-y-3 rounded border border-gray-200 bg-white p-4">
		<h2 class="font-medium">退貨門市（買家未取件時退回這裡）</h2>
		<div class="grid grid-cols-3 gap-3">
			<label class="block text-sm text-gray-700">
				超商
				<select bind:value={form.return_store.sub_type} class={input}>
					<option value="">未設定</option>
					{#each cvsTypes as t (t)}<option value={t}>{CVS_LABELS[t]}</option>{/each}
				</select>
				{#if errors['return_store.sub_type']}<span class="text-red-600">{errors['return_store.sub_type']}</span>{/if}
			</label>
			<label class="block text-sm text-gray-700">
				門市代號
				<input type="text" bind:value={form.return_store.store_id} class={input} />
				{#if errors['return_store.store_id']}<span class="text-red-600">{errors['return_store.store_id']}</span>{/if}
			</label>
			<label class="block text-sm text-gray-700">
				門市名稱
				<input type="text" bind:value={form.return_store.store_name} class={input} />
				{#if errors['return_store.store_name']}<span class="text-red-600">{errors['return_store.store_name']}</span>{/if}
			</label>
		</div>
	</section>

	<button type="submit" disabled={saving} class="rounded bg-gray-900 px-4 py-2 text-white disabled:opacity-50">
		{saving ? '儲存中…' : '儲存設定'}
	</button>
</form>
