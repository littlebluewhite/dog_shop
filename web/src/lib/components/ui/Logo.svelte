<script lang="ts">
	let { name, size = 'md', inverted = false }: { name: string; size?: 'md' | 'lg'; inverted?: boolean } = $props();

	const mark = $derived(size === 'lg' ? 40 : 28);
</script>

<span class="inline-flex items-center gap-2 {inverted ? 'text-ground' : 'text-ink'}">
	<svg width={mark} height={mark} viewBox="0 0 24 24" aria-hidden="true" class="shrink-0">
		<path
			d="M3 3h8.6a2 2 0 0 1 1.4.6l7.4 7.4a2 2 0 0 1 0 2.8l-6.6 6.6a2 2 0 0 1-2.8 0L3.6 13A2 2 0 0 1 3 11.6V3z"
			class="fill-brand"
		/>
		<circle cx="7.5" cy="7.5" r="1.6" class={inverted ? 'fill-ink' : 'fill-ground'} />
	</svg>
	<span class="font-extrabold tracking-tight {size === 'lg' ? 'text-2xl' : 'text-lg'}">{name}</span>
</span>
