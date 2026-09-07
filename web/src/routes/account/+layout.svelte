<script lang="ts">
	import { page } from '$app/state';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	const links = [
		{ href: '/account', label: '個人資料' },
		{ href: '/account/orders', label: '我的訂單' },
		{ href: '/account/addresses', label: '常用地址' }
	];

	function isActive(href: string): boolean {
		return href === '/account' ? page.url.pathname === '/account' : page.url.pathname.startsWith(href);
	}
</script>

<div class="flex flex-col gap-6 md:flex-row">
	<aside class="md:w-44 md:shrink-0">
		<nav class="flex gap-2 md:flex-col">
			{#each links as link (link.href)}
				<a href={link.href} class="rounded px-3 py-2 text-sm {isActive(link.href) ? 'bg-gray-900 text-white' : 'hover:bg-gray-200'}">{link.label}</a>
			{/each}
		</nav>
	</aside>
	<section class="min-w-0 flex-1">
		{@render children()}
	</section>
</div>
