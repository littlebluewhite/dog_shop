<script lang="ts">
	import type { InvoiceForm } from '$lib/checkout';
	import Field from '$lib/components/ui/Field.svelte';
	import { INVOICE_LABELS } from '$lib/labels';
	import type { InvoiceType } from '$lib/types';

	let { invoice = $bindable(), errors = {} }: { invoice: InvoiceForm; errors?: Record<string, string> } = $props();

	const types: InvoiceType[] = ['personal', 'company', 'donation'];
	const carriers: { value: '1' | '2' | '3'; label: string }[] = [
		{ value: '1', label: '綠界會員載具（發票寄到 Email）' },
		{ value: '2', label: '自然人憑證' },
		{ value: '3', label: '手機條碼' }
	];
</script>

<fieldset class="space-y-3">
	<legend class="sr-only">發票</legend>
	<div class="grid gap-2 sm:grid-cols-3">
		{#each types as t (t)}
			<label class="option-card"><input type="radio" bind:group={invoice.type} value={t} /> {INVOICE_LABELS[t]}</label>
		{/each}
	</div>
	{#if invoice.type === 'personal'}
		<Field label="載具">
			<select bind:value={invoice.carrier_type} class="input mt-1">
				{#each carriers as c (c.value)}<option value={c.value}>{c.label}</option>{/each}
			</select>
		</Field>
		{#if invoice.carrier_type !== '1'}
			<Field
				label={invoice.carrier_type === '2' ? '自然人憑證條碼' : '手機條碼'}
				type="text"
				bind:value={invoice.carrier_num}
				placeholder={invoice.carrier_type === '2' ? 'AB12345678901234' : '/ABC+123'}
				error={errors['invoice.carrier_num']}
			/>
		{/if}
	{:else if invoice.type === 'company'}
		<Field label="統一編號" type="text" bind:value={invoice.tax_id} inputmode="numeric" error={errors['invoice.tax_id']} />
		<Field label="發票抬頭" type="text" bind:value={invoice.title} error={errors['invoice.title']} />
		<Field label="發票地址" type="text" bind:value={invoice.address} error={errors['invoice.address']} />
	{:else}
		<Field label="愛心碼" type="text" bind:value={invoice.love_code} inputmode="numeric" placeholder="例如 168" error={errors['invoice.love_code']} />
	{/if}
</fieldset>
