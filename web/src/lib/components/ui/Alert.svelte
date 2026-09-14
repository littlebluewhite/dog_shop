<script lang="ts">
	import type { Snippet } from 'svelte';

	type Tone = 'info' | 'success' | 'warning' | 'danger';

	let {
		tone = 'info',
		title,
		class: className = '',
		children
	}: { tone?: Tone; title?: string; class?: string; children: Snippet } = $props();

	const BOX: Record<Tone, string> = {
		info: 'border-ink bg-surface',
		success: 'border-success bg-success-soft',
		warning: 'border-warning bg-warning-soft',
		danger: 'border-danger bg-danger-soft'
	};
	const TITLE: Record<Tone, string> = {
		info: 'text-ink',
		success: 'text-success',
		warning: 'text-warning',
		danger: 'text-danger'
	};
</script>

<div role={tone === 'danger' ? 'alert' : undefined} class="rounded-control border-l-4 px-4 py-3 text-sm text-ink {BOX[tone]} {className}">
	{#if title}<p class="font-semibold {TITLE[tone]}">{title}</p>{/if}
	{@render children()}
</div>
