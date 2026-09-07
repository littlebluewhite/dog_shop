<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { page } from '$app/state';
	import { api, ApiError } from '$lib/api';
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

<div class="mx-auto max-w-sm">
	<h1 class="mb-6 text-2xl font-bold">登入</h1>
	<form onsubmit={submit} class="space-y-4">
		<label class="block text-sm text-gray-700">
			Email
			<input
				type="email"
				bind:value={email}
				required
				autocomplete="email"
				class="mt-1 w-full rounded border border-gray-300 px-3 py-2 text-base"
			/>
		</label>
		<label class="block text-sm text-gray-700">
			密碼
			<input
				type="password"
				bind:value={password}
				required
				autocomplete="current-password"
				class="mt-1 w-full rounded border border-gray-300 px-3 py-2 text-base"
			/>
		</label>
		{#if error}<p class="text-sm text-red-600">{error}</p>{/if}
		<button
			type="submit"
			disabled={submitting}
			class="w-full rounded bg-gray-900 px-4 py-2 text-white disabled:opacity-50"
		>
			{submitting ? '登入中…' : '登入'}
		</button>
	</form>
	<div class="mt-4 flex justify-between text-sm text-gray-600">
		<a href="/register" class="underline">還沒有帳號？註冊</a>
		<a href="/forgot-password" class="underline">忘記密碼？</a>
	</div>
</div>
