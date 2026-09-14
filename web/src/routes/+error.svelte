<script lang="ts">
	import { page } from '$app/state';
	import Button from '$lib/components/ui/Button.svelte';
</script>

<div class="mx-auto max-w-md py-20 text-center">
	<h1 class="text-6xl font-extrabold tracking-tight tabular-nums">{page.status}</h1>
	<p class="mt-3 text-ink-soft">{page.error?.message ?? '發生錯誤'}</p>
	<div class="mt-6"><Button href="/">回首頁</Button></div>
</div>
