<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api, ApiError } from '$lib/api';
	import Alert from '$lib/components/ui/Alert.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
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

<div class="mx-auto mt-4 max-w-sm">
	<Card class="p-6 md:p-8">
		<div class="mb-6 flex flex-col items-center gap-3 text-center">
			<span class="flex h-14 w-14 items-center justify-center rounded-full bg-brand-soft text-brand"><Icon name="tag" size={28} /></span>
			<h1 class="text-2xl font-extrabold tracking-tight">重設密碼</h1>
		</div>
		<form onsubmit={submit} class="space-y-4" novalidate>
			<Field label="新密碼（至少 8 碼）" type="password" bind:value={password} required autocomplete="new-password" />
			<Field label="再輸入一次" type="password" bind:value={confirm} required autocomplete="new-password" />
			{#if error}<Alert tone="danger">{error}</Alert>{/if}
			<Button type="submit" size="lg" class="w-full" disabled={submitting}>{submitting ? '更新中…' : '更新密碼'}</Button>
		</form>
	</Card>
</div>
