<script lang="ts">
	import Icon from './Icon.svelte';
	import { clampQty } from './qty';

	let {
		value = $bindable(1),
		min = 1,
		max = 99,
		onchange,
		label = '數量'
	}: { value?: number; min?: number; max?: number; onchange?: (v: number) => void; label?: string } = $props();

	function set(next: number) {
		const v = clampQty(next, min, max, value);
		value = v;
		onchange?.(v);
	}
</script>

<div class="inline-flex h-11 shrink-0 items-center rounded-full border border-line bg-ground">
	<button type="button" class="flex h-full w-10 items-center justify-center rounded-l-full text-ink hover:bg-surface" onclick={() => set(value - 1)} aria-label="減少">
		<Icon name="minus" size={16} />
	</button>
	<input
		type="number"
		{min}
		{max}
		{value}
		onchange={(e) => {
			set(Number(e.currentTarget.value));
			e.currentTarget.value = String(value);
		}}
		aria-label={label}
		class="h-full w-12 border-x border-line bg-transparent text-center text-base font-semibold [appearance:textfield] focus:border-brand focus:ring-2 focus:ring-inset focus:ring-brand/40 focus:outline-none [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
	/>
	<button type="button" class="flex h-full w-10 items-center justify-center rounded-r-full text-ink hover:bg-surface" onclick={() => set(value + 1)} aria-label="增加">
		<Icon name="plus" size={16} />
	</button>
</div>
