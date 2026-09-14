<script lang="ts">
	import { goto, invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import Alert from '$lib/components/ui/Alert.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import Icon from '$lib/components/ui/Icon.svelte';
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

<div class="mx-auto mt-4 max-w-sm">
	<Card class="p-6 md:p-8">
		<div class="mb-6 flex flex-col items-center gap-3 text-center">
			<span class="flex h-14 w-14 items-center justify-center rounded-full bg-brand-soft text-brand"><Icon name="tag" size={28} /></span>
			<h1 class="text-2xl font-extrabold tracking-tight">註冊</h1>
		</div>
		<form onsubmit={submit} class="space-y-4" novalidate>
			<Field label="Email" type="email" bind:value={email} required autocomplete="email" error={errors.email} />
			<Field label="密碼（至少 8 碼）" type="password" bind:value={password} required autocomplete="new-password" error={errors.password} />
			<Field label="姓名" type="text" bind:value={name} required autocomplete="name" error={errors.name} />
			<Field label="手機（選填）" type="tel" bind:value={phone} autocomplete="tel" placeholder="09xxxxxxxx" error={errors.phone} />
			{#if message}<Alert tone="danger">{message}</Alert>{/if}
			<Button type="submit" size="lg" class="w-full" disabled={submitting}>{submitting ? '註冊中…' : '建立帳號'}</Button>
		</form>
		<p class="mt-5 text-center text-sm">已經有帳號？<a href="/login" class="link">登入</a></p>
	</Card>
</div>
