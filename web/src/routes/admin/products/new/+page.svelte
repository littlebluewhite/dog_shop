<script lang="ts">
	import ProductForm from '$lib/components/admin/ProductForm.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head><title>新增商品</title></svelte:head>

<h1 class="text-2xl font-bold">新增商品</h1>
<div class="mt-4">
	<ProductForm categories={data.categories} />
</div>
