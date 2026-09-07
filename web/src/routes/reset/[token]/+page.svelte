<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, ApiError } from '$lib/api';
	import { toast } from '$lib/toast.svelte';

	let password = $state('');
	let confirm = $state('');
	let error = $state('');
	let submitting = $state(false);

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		if (password !== confirm) {
			error = '兩次輸入的密碼不一樣';
			return;
		}
		submitting = true;
		try {
			await api('/api/auth/reset', {
				method: 'POST',
				body: JSON.stringify({ token: page.params.token, password })
			});
			toast.show('密碼已更新，請重新登入');
			await goto('/login');
		} catch (err) {
			error = err instanceof ApiError ? (err.field('token') ?? err.field('password') ?? err.message) : '重設失敗，請再試一次';
		} finally {
			submitting = false;
		}
	}
</script>

<svelte:head><title>重設密碼</title></svelte:head>

<div class="mx-auto max-w-sm">
	<h1 class="mb-6 text-2xl font-bold">重設密碼</h1>
	<form onsubmit={submit} class="space-y-4" novalidate>
		<label class="block text-sm text-gray-700">
			新密碼（至少 8 碼）
			<input type="password" bind:value={password} required autocomplete="new-password" class="mt-1 w-full rounded border border-gray-300 px-3 py-2 text-base" />
		</label>
		<label class="block text-sm text-gray-700">
			再輸入一次
			<input type="password" bind:value={confirm} required autocomplete="new-password" class="mt-1 w-full rounded border border-gray-300 px-3 py-2 text-base" />
		</label>
		{#if error}<p class="text-sm text-red-600">{error}</p>{/if}
		<button type="submit" disabled={submitting} class="w-full rounded bg-gray-900 px-4 py-2 text-white disabled:opacity-50">
			{submitting ? '更新中…' : '更新密碼'}
		</button>
	</form>
</div>
