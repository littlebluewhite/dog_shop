<script lang="ts">
	import type { HomeAddressInput } from '$lib/types';
	import Field from '$lib/components/ui/Field.svelte';
	import { cities, districts, postalCode } from '$lib/tw-address';

	let {
		address = $bindable(),
		errors = {},
		prefix = ''
	}: { address: HomeAddressInput; errors?: Record<string, string>; prefix?: string } = $props();

	const cityOptions = cities();
	const districtOptions = $derived(districts(address.city));

	function onCity() {
		address.district = '';
		address.postal_code = '';
	}
	function onDistrict() {
		address.postal_code = postalCode(address.city, address.district);
	}
	function err(key: string): string | undefined {
		return errors[prefix + key];
	}
</script>

<div class="grid grid-cols-3 gap-3">
	<Field label="縣市" error={err('city')}>
		{#snippet children({ errorId, invalid })}
			<select bind:value={address.city} onchange={onCity} class="input mt-1" aria-invalid={invalid} aria-describedby={errorId}>
				<option value="">請選擇</option>
				{#each cityOptions as c (c)}<option value={c}>{c}</option>{/each}
			</select>
		{/snippet}
	</Field>
	<Field label="鄉鎮市區" error={err('district')}>
		{#snippet children({ errorId, invalid })}
			<select bind:value={address.district} onchange={onDistrict} class="input mt-1" aria-invalid={invalid} aria-describedby={errorId}>
				<option value="">請選擇</option>
				{#each districtOptions as d (d)}<option value={d}>{d}</option>{/each}
			</select>
		{/snippet}
	</Field>
	<Field label="郵遞區號" type="text" bind:value={address.postal_code} inputmode="numeric" error={err('postal_code')} />
</div>
<Field class="mt-3" label="地址" type="text" bind:value={address.street} autocomplete="street-address" error={err('street')} />
