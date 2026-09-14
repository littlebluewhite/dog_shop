<script lang="ts">
	import { page } from '$app/state';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	const links = [
		{ href: '/admin', label: '儀表板' },
		{ href: '/admin/orders', label: '訂單' },
		{ href: '/admin/products', label: '商品' },
		{ href: '/admin/import', label: '匯入' },
		{ href: '/admin/categories', label: '分類' },
		{ href: '/admin/settings', label: '設定' }
	];

	function isActive(href: string): boolean {
		return href === '/admin' ? page.url.pathname === '/admin' : page.url.pathname.startsWith(href);
	}
</script>

<div class="flex flex-col gap-6 md:flex-row md:gap-8">
	<aside class="md:w-44 md:shrink-0">
		<nav class="-mx-4 flex gap-1 overflow-x-auto px-4 md:mx-0 md:flex-col md:px-0" aria-label="後台">
			{#each links as link (link.href)}
				<a
					href={link.href}
					class="shrink-0 rounded-full px-4 py-2 text-sm transition-colors duration-150 {isActive(link.href) ? 'bg-brand-soft font-bold text-ink' : 'font-medium text-ink-soft hover:bg-surface'}"
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
