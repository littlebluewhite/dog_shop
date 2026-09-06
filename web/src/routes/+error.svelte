<script lang="ts">
	import { page } from '$app/state';
</script>

<div class="mx-auto max-w-md py-20 text-center">
	<h1 class="text-4xl font-bold">{page.status}</h1>
	<p class="mt-3 text-gray-600">{page.error?.message ?? '發生錯誤'}</p>
	<a href="/" class="mt-6 inline-block rounded bg-gray-900 px-4 py-2 text-white">回首頁</a>
</div>
