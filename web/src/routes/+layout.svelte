<script lang="ts">
	import '../app.css';
	import { onMount } from 'svelte';
	import { goto, invalidateAll } from '$app/navigation';
	import { page } from '$app/state';
	import { api } from '$lib/api';
	import favicon from '$lib/assets/favicon.svg';
	import { cart } from '$lib/cart.svelte';
	import Toasts from '$lib/components/Toasts.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
	import Logo from '$lib/components/ui/Logo.svelte';
	import type { LayoutProps } from './$types';

	let { data, children }: LayoutProps = $props();

	onMount(() => cart.load());

	async function logout() {
		await api('/api/auth/logout', { method: 'POST' });
		await invalidateAll();
		await goto('/');
	}

	const navLink =
		'inline-flex h-10 items-center gap-1.5 rounded-full px-2 whitespace-nowrap text-sm font-medium text-ink transition-colors duration-150 hover:bg-surface md:px-3';
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<!-- 第一層頁若用元件 bind:（如 /checkout）走 SSR settle loop，自己的 <title> 會被這裡蓋掉；
	     那些頁改由 load 回傳 title，見 fix-wave 修正波第 5 項的 <title> 診斷 -->
	<title>{page.data.title ?? data.shop.name}</title>
</svelte:head>

<header class="sticky top-0 z-40 border-b border-line bg-ground/95 backdrop-blur">
	<div class="mx-auto flex h-16 max-w-6xl items-center gap-3 px-4 md:px-6">
		<a href="/" class="rounded-full"><Logo name={data.shop.name} /></a>
		<nav class="ml-auto flex items-center gap-0.5 md:gap-1" aria-label="主選單">
			<a href="/products" class={navLink}>全部商品</a>
			<a href="/cart" class="{navLink} relative">
				<Icon name="cart" />
				<span class="sr-only md:not-sr-only">購物車</span>
				{#if cart.count > 0}
					<span class="absolute top-0 right-0 min-w-5 rounded-full bg-brand px-1.5 text-center text-xs leading-5 font-bold text-ink md:static md:ml-0.5">{cart.count}</span>
				{/if}
			</a>
			{#if data.user}
				{#if data.user.role === 'admin'}
					<a href="/admin" class={navLink}><Icon name="user" /><span class="sr-only md:not-sr-only">後台</span></a>
				{/if}
				{#if data.user.role !== 'admin'}
					<a href="/account" class={navLink}><Icon name="user" /><span class="sr-only md:not-sr-only">會員中心</span></a>
				{/if}
				<button type="button" class={navLink} onclick={logout}>登出</button>
			{:else}
				<a href="/login" class={navLink}>登入</a>
			{/if}
		</nav>
	</div>
</header>

<main class="mx-auto min-h-[60vh] max-w-6xl px-4 py-6 md:px-6 md:py-8">
	{@render children()}
</main>

<footer class="mt-16 bg-ink text-ground">
	<div class="mx-auto flex max-w-6xl flex-col gap-8 px-4 py-10 md:flex-row md:items-start md:justify-between md:px-6">
		<div>
			<Logo name={data.shop.name} inverted />
			<div class="mt-3 space-y-1 text-sm text-ground/80">
				{#if data.shop.contact_email}<p>{data.shop.contact_email}</p>{/if}
				{#if data.shop.contact_phone}<p>{data.shop.contact_phone}</p>{/if}
			</div>
		</div>
		<!-- 深藍底上全域的 ink 焦點框看不到，改用白框 -->
		<nav class="flex flex-col gap-2 text-sm text-ground/80 md:items-end" aria-label="頁尾選單">
			<a href="/products" class="hover:text-ground hover:underline focus-visible:outline-ground">全部商品</a>
			<a href="/cart" class="hover:text-ground hover:underline focus-visible:outline-ground">購物車</a>
			<a href="/account" class="hover:text-ground hover:underline focus-visible:outline-ground">會員中心</a>
		</nav>
	</div>
</footer>

<Toasts />
