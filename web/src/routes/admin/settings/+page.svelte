<script lang="ts">
	import { untrack } from 'svelte';
	import { invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
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

<PageHeader title="商店設定" />

<form onsubmit={save} class="mt-6 max-w-2xl space-y-6" novalidate>
	<Card title="商店資訊">
		<div class="space-y-3">
			<Field label="商店名稱" type="text" bind:value={form.shop.name} error={errors['shop.name']} />
			<Field label="簡介（選填）" error={errors['shop.description']}>
				<textarea bind:value={form.shop.description} rows="3" class="input mt-1"></textarea>
			</Field>
			<div class="grid grid-cols-2 gap-3">
				<Field label="聯絡 Email" type="email" bind:value={form.shop.contact_email} error={errors['shop.contact_email']} />
				<Field label="聯絡電話" type="text" bind:value={form.shop.contact_phone} error={errors['shop.contact_phone']} />
			</div>
		</div>
	</Card>
	<Card title="運費">
		<div class="grid grid-cols-3 gap-3">
			<Field label="超商取貨（元）" type="number" min="0" bind:value={form.shipping.cvs_fee} error={errors['shipping.cvs_fee']} />
			<Field label="宅配（元）" type="number" min="0" bind:value={form.shipping.home_fee} error={errors['shipping.home_fee']} />
			<Field label="免運門檻（元，0 = 不免運）" type="number" min="0" bind:value={form.shipping.free_threshold} error={errors['shipping.free_threshold']} />
		</div>
		<p class="mt-3 text-xs text-ink-soft">免運以商品小計比較；超商取貨商品小計上限 20,000 元（綠界規定）。</p>
	</Card>
	<Card title="付款方式">
		<div class="flex flex-wrap gap-4 text-sm">
			<label class="flex items-center gap-2"><input type="checkbox" bind:checked={form.payment_methods.credit} class="accent-brand" /> 信用卡</label>
			<label class="flex items-center gap-2"><input type="checkbox" bind:checked={form.payment_methods.atm} class="accent-brand" /> ATM 轉帳</label>
			<label class="flex items-center gap-2"><input type="checkbox" bind:checked={form.payment_methods.cvs_code} class="accent-brand" /> 超商代碼繳費</label>
		</div>
		{#if errors.payment_methods}<p class="field-error">{errors.payment_methods}</p>{/if}
	</Card>
	<Card title="寄件人（超商物流單用）">
		<div class="grid grid-cols-2 gap-3">
			<Field label="姓名" type="text" bind:value={form.sender.name} error={errors['sender.name']} />
			<Field label="手機" type="tel" bind:value={form.sender.phone} placeholder="09xxxxxxxx" error={errors['sender.phone']} />
		</div>
	</Card>
	<Card title="退貨門市（買家未取件時退回這裡）">
		<div class="grid grid-cols-3 gap-3">
			<Field label="超商" error={errors['return_store.sub_type']}>
				<select bind:value={form.return_store.sub_type} class="input mt-1">
					<option value="">未設定</option>
					{#each cvsTypes as t (t)}<option value={t}>{CVS_LABELS[t]}</option>{/each}
				</select>
			</Field>
			<Field label="門市代號" type="text" bind:value={form.return_store.store_id} error={errors['return_store.store_id']} />
			<Field label="門市名稱" type="text" bind:value={form.return_store.store_name} error={errors['return_store.store_name']} />
		</div>
	</Card>
	<Button type="submit" disabled={saving}>{saving ? '儲存中…' : '儲存設定'}</Button>
</form>
