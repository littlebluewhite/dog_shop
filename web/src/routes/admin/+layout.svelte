<script lang="ts">
	import { page } from '$app/state';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	const links = [
		{ href: '/admin', label: '儀表板' },
		{ href: '/admin/orders', label: '訂單' },
		{ href: '/admin/products', label: '商品' },
		{ href: '/admin/categories', label: '分類' },
		{ href: '/admin/settings', label: '設定' }
	];

	function isActive(href: string): boolean {
		return href === '/admin' ? page.url.pathname === '/admin' : page.url.pathname.startsWith(href);
	}
</script>

<div class="flex flex-col gap-6 md:flex-row">
	<aside class="md:w-44 md:shrink-0">
		<nav class="flex gap-2 md:flex-col">
			{#each links as link (link.href)}
				<a
					href={link.href}
					class="rounded px-3 py-2 text-sm {isActive(link.href) ? 'bg-gray-900 text-white' : 'hover:bg-gray-200'}"
				>
					{link.label}
				</a>
			{/each}
		</nav>
	</aside>
	<section class="min-w-0 flex-1">
		{@render children()}
	</section>
</div>
