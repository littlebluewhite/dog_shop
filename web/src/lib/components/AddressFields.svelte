<script lang="ts">
	import type { HomeAddressInput } from '$lib/types';
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
	<label class="block text-sm text-gray-700">
		縣市
		<select bind:value={address.city} onchange={onCity} class="mt-1 w-full rounded border border-gray-300 px-2 py-2">
			<option value="">請選擇</option>
			{#each cityOptions as c (c)}<option value={c}>{c}</option>{/each}
		</select>
		{#if err('city')}<span class="text-red-600">{err('city')}</span>{/if}
	</label>
	<label class="block text-sm text-gray-700">
		鄉鎮市區
		<select bind:value={address.district} onchange={onDistrict} class="mt-1 w-full rounded border border-gray-300 px-2 py-2">
			<option value="">請選擇</option>
			{#each districtOptions as d (d)}<option value={d}>{d}</option>{/each}
		</select>
		{#if err('district')}<span class="text-red-600">{err('district')}</span>{/if}
	</label>
	<label class="block text-sm text-gray-700">
		郵遞區號
		<input type="text" bind:value={address.postal_code} inputmode="numeric" class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
		{#if err('postal_code')}<span class="text-red-600">{err('postal_code')}</span>{/if}
	</label>
</div>
<label class="mt-3 block text-sm text-gray-700">
	地址
	<input type="text" bind:value={address.street} autocomplete="street-address" class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
	{#if err('street')}<span class="text-red-600">{err('street')}</span>{/if}
</label>
