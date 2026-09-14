<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { BadgeTone } from './status';

	let { tone = 'neutral', class: className = '', children }: { tone?: BadgeTone; class?: string; children: Snippet } = $props();

	const TONES: Record<BadgeTone, string> = {
		brand: 'bg-brand text-ink',
		neutral: 'bg-surface text-ink-soft',
		success: 'bg-success-soft text-success',
		warning: 'bg-warning-soft text-warning',
		danger: 'bg-danger-soft text-danger'
	};
</script>

<span class="inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-semibold whitespace-nowrap {TONES[tone]} {className}">{@render children()}</span>
