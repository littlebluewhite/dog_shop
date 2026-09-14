<script lang="ts">
	import { api, ApiError } from '$lib/api';
	import Alert from '$lib/components/ui/Alert.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';

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

<div class="mx-auto mt-4 max-w-sm">
	<Card class="p-6 md:p-8">
		<div class="mb-6 flex flex-col items-center gap-3 text-center">
			<span class="flex h-14 w-14 items-center justify-center rounded-full bg-brand-soft text-brand"><Icon name="tag" size={28} /></span>
			<h1 class="text-2xl font-extrabold tracking-tight">忘記密碼</h1>
		</div>
		{#if sent}
			<Alert tone="success">如果這個 Email 有註冊過，我們會寄出重設密碼的連結（1 小時內有效），請到信箱查看。</Alert>
		{:else}
			<form onsubmit={submit} class="space-y-4" novalidate>
				<Field label="註冊時用的 Email" type="email" bind:value={email} required autocomplete="email" />
				{#if error}<Alert tone="danger">{error}</Alert>{/if}
				<Button type="submit" size="lg" class="w-full" disabled={submitting}>{submitting ? '送出中…' : '寄送重設連結'}</Button>
			</form>
		{/if}
		<p class="mt-5 text-center text-sm"><a href="/login" class="link">回登入</a></p>
	</Card>
</div>
