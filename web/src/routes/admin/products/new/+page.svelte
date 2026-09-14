<script lang="ts">
	import ProductForm from '$lib/components/admin/ProductForm.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head><title>新增商品</title></svelte:head>

<PageHeader title="新增商品" />
<div class="mt-5">
	<ProductForm categories={data.categories} />
</div>
