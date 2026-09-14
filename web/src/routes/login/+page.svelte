<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { page } from '$app/state';
	import { api, ApiError } from '$lib/api';
	import Alert from '$lib/components/ui/Alert.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
	import type { User } from '$lib/types';

	let email = $state('');
	let password = $state('');
	let error = $state('');
	let submitting = $state(false);

	function safeRedirect(target: string | null, user: User): string {
		if (target && /^\/(?![\/\\])/.test(target)) return target;
		return user.role === 'admin' ? '/admin' : '/';
	}

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		submitting = true;
		try {
			const res = await api<{ user: User }>('/api/auth/login', {
				method: 'POST',
				body: JSON.stringify({ email, password })
			});
			await invalidateAll();
			await goto(safeRedirect(page.url.searchParams.get('redirect'), res.user));
		} catch (err) {
			error = err instanceof ApiError ? err.message : '登入失敗，請再試一次';
		} finally {
			submitting = false;
		}
	}
</script>

<svelte:head><title>登入</title></svelte:head>

<div class="mx-auto mt-4 max-w-sm">
	<Card class="p-6 md:p-8">
		<div class="mb-6 flex flex-col items-center gap-3 text-center">
			<span class="flex h-14 w-14 items-center justify-center rounded-full bg-brand-soft text-brand"><Icon name="tag" size={28} /></span>
			<h1 class="text-2xl font-extrabold tracking-tight">登入</h1>
		</div>
		<form onsubmit={submit} class="space-y-4">
			<Field label="Email" type="email" bind:value={email} required autocomplete="email" />
			<Field label="密碼" type="password" bind:value={password} required autocomplete="current-password" />
			{#if error}<Alert tone="danger">{error}</Alert>{/if}
			<Button type="submit" size="lg" class="w-full" disabled={submitting}>{submitting ? '登入中…' : '登入'}</Button>
		</form>
		<div class="mt-5 flex justify-between text-sm">
			<a href="/register" class="link">還沒有帳號？註冊</a>
			<a href="/forgot-password" class="link">忘記密碼？</a>
		</div>
	</Card>
</div>
