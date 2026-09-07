<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import type { User } from '$lib/types';

	let email = $state('');
	let password = $state('');
	let name = $state('');
	let phone = $state('');
	let errors = $state<Record<string, string>>({});
	let message = $state('');
	let submitting = $state(false);

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		errors = {};
		message = '';
		submitting = true;
		try {
			await api<{ user: User }>('/api/auth/register', {
				method: 'POST',
				body: JSON.stringify({ email, password, name, phone: phone.trim() || null })
			});
			await invalidateAll();
			await goto('/');
		} catch (err) {
			if (err instanceof ApiError) {
				errors = err.fields();
				if (Object.keys(errors).length === 0) message = err.message;
			} else {
				message = '註冊失敗，請再試一次';
			}
		} finally {
			submitting = false;
		}
	}
</script>

<svelte:head><title>註冊</title></svelte:head>

<div class="mx-auto max-w-sm">
	<h1 class="mb-6 text-2xl font-bold">註冊</h1>
	<form onsubmit={submit} class="space-y-4" novalidate>
		<label class="block text-sm text-gray-700">
			Email
			<input type="email" bind:value={email} required autocomplete="email" class="mt-1 w-full rounded border border-gray-300 px-3 py-2 text-base" />
			{#if errors.email}<span class="text-red-600">{errors.email}</span>{/if}
		</label>
		<label class="block text-sm text-gray-700">
			密碼（至少 8 碼）
			<input type="password" bind:value={password} required autocomplete="new-password" class="mt-1 w-full rounded border border-gray-300 px-3 py-2 text-base" />
			{#if errors.password}<span class="text-red-600">{errors.password}</span>{/if}
		</label>
		<label class="block text-sm text-gray-700">
			姓名
			<input type="text" bind:value={name} required autocomplete="name" class="mt-1 w-full rounded border border-gray-300 px-3 py-2 text-base" />
			{#if errors.name}<span class="text-red-600">{errors.name}</span>{/if}
		</label>
		<label class="block text-sm text-gray-700">
			手機（選填）
			<input type="tel" bind:value={phone} autocomplete="tel" placeholder="09xxxxxxxx" class="mt-1 w-full rounded border border-gray-300 px-3 py-2 text-base" />
			{#if errors.phone}<span class="text-red-600">{errors.phone}</span>{/if}
		</label>
		{#if message}<p class="text-sm text-red-600">{message}</p>{/if}
		<button type="submit" disabled={submitting} class="w-full rounded bg-gray-900 px-4 py-2 text-white disabled:opacity-50">
			{submitting ? '註冊中…' : '建立帳號'}
		</button>
	</form>
	<p class="mt-4 text-sm text-gray-600">已經有帳號？<a href="/login" class="underline">登入</a></p>
</div>
