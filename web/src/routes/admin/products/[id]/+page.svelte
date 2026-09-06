<script lang="ts">
	import ProductForm from '$lib/components/admin/ProductForm.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head><title>編輯商品</title></svelte:head>

<h1 class="text-2xl font-bold">編輯商品</h1>
<p class="mt-1 text-sm text-gray-500">
	前台網址：<a href={`/products/${data.product.slug}`} class="underline" target="_blank" rel="noreferrer">/products/{data.product.slug}</a>（要上架才看得到）
</p>
<div class="mt-4">
	<!-- 存檔後 updated_at 會變，用 key 讓表單重新讀取伺服器回來的資料（新規格才會有 id） -->
	{#key data.product.updated_at}
		<ProductForm product={data.product} categories={data.categories} />
	{/key}
</div>
