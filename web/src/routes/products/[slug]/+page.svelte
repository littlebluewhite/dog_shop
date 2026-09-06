<script lang="ts">
	import { page } from '$app/state';
	import ProductView from '$lib/components/ProductView.svelte';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	const p = $derived(data.product);
	const absolute = (path: string) => `${page.url.origin}${path}`;
	const summary = $derived(p.description.replace(/\s+/g, ' ').trim().slice(0, 150));
	const prices = $derived(p.variants.map((v) => v.price));
	// JSON-LD Product（規格 §6.1）。把 < 換成 \u003c，避免描述裡的字串提早關掉 script 標籤
	const jsonLd = $derived(
		JSON.stringify({
			'@context': 'https://schema.org',
			'@type': 'Product',
			name: p.name,
			description: p.description,
			image: p.images.map((i) => absolute(i.path)),
			offers: {
				'@type': 'AggregateOffer',
				priceCurrency: 'TWD',
				lowPrice: prices.length ? Math.min(...prices) : 0,
				highPrice: prices.length ? Math.max(...prices) : 0,
				offerCount: p.variants.length,
				availability: p.variants.some((v) => v.stock > 0) ? 'https://schema.org/InStock' : 'https://schema.org/OutOfStock'
			}
		}).replace(/</g, '\\u003c')
	);
</script>

<svelte:head>
	<title>{p.name}</title>
	<meta name="description" content={summary} />
	<meta property="og:type" content="product" />
	<meta property="og:title" content={p.name} />
	<meta property="og:description" content={summary} />
	<meta property="og:url" content={absolute(`/products/${p.slug}`)} />
	{#if p.images[0]}<meta property="og:image" content={absolute(p.images[0].path)} />{/if}
	{@html `<script type="application/ld+json">${jsonLd}</script>`}
</svelte:head>

{#key p.id}
	<ProductView product={p} />
{/key}
