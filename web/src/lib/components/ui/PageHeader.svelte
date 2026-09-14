<script lang="ts">
	import type { Snippet } from 'svelte';

	let { title, subtitle, children }: { title: string; subtitle?: string; children?: Snippet } = $props();
</script>

<div class="flex flex-wrap items-end justify-between gap-3">
	<div>
		<h1 class="text-2xl font-extrabold tracking-tight md:text-[28px]">{title}</h1>
		{#if subtitle}<p class="mt-1 text-sm text-ink-soft">{subtitle}</p>{/if}
	</div>
	{#if children}<div class="flex flex-wrap items-center gap-2">{@render children()}</div>{/if}
</div>
