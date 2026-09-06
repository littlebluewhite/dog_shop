<script lang="ts">
	import { page as currentPage } from '$app/state';

	let { page, perPage, total }: { page: number; perPage: number; total: number } = $props();

	const pages = $derived(Math.max(1, Math.ceil(total / perPage)));

	function hrefFor(target: number): string {
		const url = new URL(currentPage.url);
		url.searchParams.set('page', String(target));
		return url.pathname + url.search;
	}
</script>

{#if pages > 1}
	<nav class="mt-6 flex items-center justify-center gap-4 text-sm" aria-label="分頁">
		{#if page > 1}
			<a href={hrefFor(page - 1)} class="rounded border border-gray-300 px-3 py-1">上一頁</a>
		{/if}
		<span>第 {page} / {pages} 頁</span>
		{#if page < pages}
			<a href={hrefFor(page + 1)} class="rounded border border-gray-300 px-3 py-1">下一頁</a>
		{/if}
	</nav>
{/if}
