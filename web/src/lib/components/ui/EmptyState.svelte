<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from './Icon.svelte';

	let { message, children }: { message: string; children?: Snippet } = $props();
</script>

<div class="flex flex-col items-center gap-3 rounded-card bg-surface px-6 py-12 text-center">
	<Icon name="tag" size={32} class="text-ink-soft" />
	<p class="text-ink-soft">{message}</p>
	{#if children}<div class="mt-1">{@render children()}</div>{/if}
</div>
