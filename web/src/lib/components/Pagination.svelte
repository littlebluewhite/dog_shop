<script lang="ts">
	import { page as currentPage } from '$app/state';
	import Button from '$lib/components/ui/Button.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';

	let { page, perPage, total }: { page: number; perPage: number; total: number } = $props();

	const pages = $derived(Math.max(1, Math.ceil(total / perPage)));

	function hrefFor(target: number): string {
		const url = new URL(currentPage.url);
		url.searchParams.set('page', String(target));
		return url.pathname + url.search;
	}
</script>

{#if pages > 1}
	<nav class="mt-8 flex items-center justify-center gap-3 text-sm" aria-label="分頁">
		{#if page > 1}
			<Button variant="secondary" size="sm" href={hrefFor(page - 1)}><Icon name="chevron-left" size={16} />上一頁</Button>
		{/if}
		<span class="text-ink-soft tabular-nums">第 {page} / {pages} 頁</span>
		{#if page < pages}
			<Button variant="secondary" size="sm" href={hrefFor(page + 1)}>下一頁<Icon name="chevron-right" size={16} /></Button>
		{/if}
	</nav>
{/if}
