<script lang="ts">
	import '../app.css';
	import { goto, invalidateAll } from '$app/navigation';
	import { api } from '$lib/api';
	import favicon from '$lib/assets/favicon.svg';
	import Toasts from '$lib/components/Toasts.svelte';
	import type { LayoutProps } from './$types';

	let { data, children }: LayoutProps = $props();

	async function logout() {
		await api('/api/auth/logout', { method: 'POST' });
		await invalidateAll();
		await goto('/');
	}
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>{data.shop.name}</title>
</svelte:head>

<header class="border-b border-gray-200 bg-white">
	<div class="mx-auto flex max-w-6xl items-center gap-4 px-4 py-3">
		<a href="/" class="text-lg font-bold">{data.shop.name}</a>
		<nav class="ml-auto flex items-center gap-4 text-sm">
			<a href="/products" class="hover:underline">全部商品</a>
			{#if data.user}
				{#if data.user.role === 'admin'}
					<a href="/admin" class="hover:underline">後台</a>
				{/if}
				<button type="button" class="hover:underline" onclick={logout}>登出</button>
			{:else}
				<a href="/login" class="hover:underline">登入</a>
			{/if}
		</nav>
	</div>
</header>

<main class="mx-auto min-h-[60vh] max-w-6xl px-4 py-6">
	{@render children()}
</main>

<footer class="mt-12 border-t border-gray-200 py-8 text-center text-sm text-gray-500">
	<p>{data.shop.name}</p>
	{#if data.shop.contact_email}<p>{data.shop.contact_email}</p>{/if}
</footer>

<Toasts />
