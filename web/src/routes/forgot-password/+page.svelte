<script lang="ts">
	import { api, ApiError } from '$lib/api';

	let email = $state('');
	let error = $state('');
	let sent = $state(false);
	let submitting = $state(false);

	async function submit(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		submitting = true;
		try {
			await api<{ ok: boolean }>('/api/auth/forgot', { method: 'POST', body: JSON.stringify({ email }) });
			sent = true;
		} catch (err) {
			error = err instanceof ApiError ? (err.field('email') ?? err.message) : '送出失敗，請再試一次';
		} finally {
			submitting = false;
		}
	}
</script>

<svelte:head><title>忘記密碼</title></svelte:head>

<div class="mx-auto max-w-sm">
	<h1 class="mb-6 text-2xl font-bold">忘記密碼</h1>
	{#if sent}
		<p class="rounded border border-green-200 bg-green-50 p-4 text-sm text-green-800">
			如果這個 Email 有註冊過，我們會寄出重設密碼的連結（1 小時內有效），請到信箱查看。
		</p>
	{:else}
		<form onsubmit={submit} class="space-y-4" novalidate>
			<label class="block text-sm text-gray-700">
				註冊時用的 Email
				<input type="email" bind:value={email} required autocomplete="email" class="mt-1 w-full rounded border border-gray-300 px-3 py-2 text-base" />
			</label>
			{#if error}<p class="text-sm text-red-600">{error}</p>{/if}
			<button type="submit" disabled={submitting} class="w-full rounded bg-gray-900 px-4 py-2 text-white disabled:opacity-50">
				{submitting ? '送出中…' : '寄送重設連結'}
			</button>
		</form>
	{/if}
	<p class="mt-4 text-sm text-gray-600"><a href="/login" class="underline">回登入</a></p>
</div>
