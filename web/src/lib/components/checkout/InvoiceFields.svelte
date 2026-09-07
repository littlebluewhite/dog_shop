<script lang="ts">
	import type { InvoiceForm } from '$lib/checkout';
	import { INVOICE_LABELS } from '$lib/labels';
	import type { InvoiceType } from '$lib/types';

	let { invoice = $bindable(), errors = {} }: { invoice: InvoiceForm; errors?: Record<string, string> } = $props();

	const types: InvoiceType[] = ['personal', 'company', 'donation'];
	const carriers: { value: '1' | '2' | '3'; label: string }[] = [
		{ value: '1', label: '綠界會員載具（發票寄到 Email）' },
		{ value: '2', label: '自然人憑證' },
		{ value: '3', label: '手機條碼' }
	];
	const input = 'mt-1 w-full rounded border border-gray-300 px-3 py-2';
</script>

<fieldset class="space-y-3">
	<legend class="text-sm font-medium">發票</legend>
	<div class="flex flex-wrap gap-4 text-sm">
		{#each types as t (t)}
			<label class="flex items-center gap-2"><input type="radio" bind:group={invoice.type} value={t} /> {INVOICE_LABELS[t]}</label>
		{/each}
	</div>
	{#if invoice.type === 'personal'}
		<label class="block text-sm text-gray-700">
			載具
			<select bind:value={invoice.carrier_type} class={input}>
				{#each carriers as c (c.value)}<option value={c.value}>{c.label}</option>{/each}
			</select>
		</label>
		{#if invoice.carrier_type !== '1'}
			<label class="block text-sm text-gray-700">
				{invoice.carrier_type === '2' ? '自然人憑證條碼' : '手機條碼'}
				<input type="text" bind:value={invoice.carrier_num} placeholder={invoice.carrier_type === '2' ? 'AB12345678901234' : '/ABC+123'} class={input} />
				{#if errors['invoice.carrier_num']}<span class="text-red-600">{errors['invoice.carrier_num']}</span>{/if}
			</label>
		{/if}
	{:else if invoice.type === 'company'}
		<label class="block text-sm text-gray-700">
			統一編號
			<input type="text" bind:value={invoice.tax_id} inputmode="numeric" class={input} />
			{#if errors['invoice.tax_id']}<span class="text-red-600">{errors['invoice.tax_id']}</span>{/if}
		</label>
		<label class="block text-sm text-gray-700">
			發票抬頭
			<input type="text" bind:value={invoice.title} class={input} />
			{#if errors['invoice.title']}<span class="text-red-600">{errors['invoice.title']}</span>{/if}
		</label>
		<label class="block text-sm text-gray-700">
			發票地址
			<input type="text" bind:value={invoice.address} class={input} />
			{#if errors['invoice.address']}<span class="text-red-600">{errors['invoice.address']}</span>{/if}
		</label>
	{:else}
		<label class="block text-sm text-gray-700">
			愛心碼
			<input type="text" bind:value={invoice.love_code} inputmode="numeric" placeholder="例如 168" class={input} />
			{#if errors['invoice.love_code']}<span class="text-red-600">{errors['invoice.love_code']}</span>{/if}
		</label>
	{/if}
</fieldset>
