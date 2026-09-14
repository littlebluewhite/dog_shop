<script lang="ts">
	import type { Snippet } from 'svelte';

	let {
		title,
		step,
		class: className = '',
		children,
		actions
	}: { title?: string; step?: number; class?: string; children: Snippet; actions?: Snippet } = $props();
</script>

<section class="card {className}">
	{#if title || actions}
		<div class="mb-3 flex flex-wrap items-center justify-between gap-2">
			{#if title}
				<h2 class="flex items-center gap-2 text-lg font-bold">
					{#if step !== undefined}
						<span class="flex h-7 w-7 items-center justify-center rounded-full bg-brand text-sm font-extrabold text-ink" aria-hidden="true">{step}</span>
					{/if}
					{title}
				</h2>
			{/if}
			{#if actions}<div class="flex flex-wrap items-center gap-2">{@render actions()}</div>{/if}
		</div>
	{/if}
	{@render children()}
</section>
