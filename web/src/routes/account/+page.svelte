<script lang="ts">
	import { untrack } from 'svelte';
	import { invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import { toast } from '$lib/toast.svelte';
	import type { User } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	// 初始值刻意只取一次；存檔後 invalidateAll 會更新 data.user，但表單保留使用者剛存的值
	let name = $state(untrack(() => data.user?.name ?? ''));
	let phone = $state(untrack(() => data.user?.phone ?? ''));
	let currentPassword = $state('');
	let newPassword = $state('');
	let errors = $state<Record<string, string>>({});
	let saving = $state(false);

	async function save(e: SubmitEvent) {
		e.preventDefault();
		errors = {};
		saving = true;
		try {
			const body: Record<string, string | null> = { name, phone: phone.trim() || null };
			if (newPassword) {
				body.current_password = currentPassword;
				body.new_password = newPassword;
			}
			await api<{ user: User }>('/api/me/profile', { method: 'PUT', body: JSON.stringify(body) });
			currentPassword = '';
			newPassword = '';
			await invalidateAll();
			toast.show('已儲存');
		} catch (err) {
			if (err instanceof ApiError) {
				errors = err.fields();
				if (Object.keys(errors).length === 0) toast.show(err.message);
			} else toast.show('儲存失敗');
		} finally {
			saving = false;
		}
	}
</script>

<svelte:head><title>個人資料</title></svelte:head>

<h1 class="text-2xl font-bold">個人資料</h1>
<p class="mt-1 text-sm text-gray-500">{data.user?.email}</p>

<form onsubmit={save} class="mt-6 max-w-md space-y-4" novalidate>
	<label class="block text-sm text-gray-700">
		姓名
		<input type="text" bind:value={name} class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
		{#if errors.name}<span class="text-red-600">{errors.name}</span>{/if}
	</label>
	<label class="block text-sm text-gray-700">
		手機
		<input type="tel" bind:value={phone} placeholder="09xxxxxxxx" class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
		{#if errors.phone}<span class="text-red-600">{errors.phone}</span>{/if}
	</label>

	<fieldset class="rounded border border-gray-200 p-4">
		<legend class="px-1 text-sm text-gray-600">更改密碼（不改就留空）</legend>
		<label class="block text-sm text-gray-700">
			目前密碼
			<input type="password" bind:value={currentPassword} autocomplete="current-password" class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
			{#if errors.current_password}<span class="text-red-600">{errors.current_password}</span>{/if}
		</label>
		<label class="mt-3 block text-sm text-gray-700">
			新密碼（至少 8 碼）
			<input type="password" bind:value={newPassword} autocomplete="new-password" class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
			{#if errors.new_password}<span class="text-red-600">{errors.new_password}</span>{/if}
		</label>
	</fieldset>

	<button type="submit" disabled={saving} class="rounded bg-gray-900 px-4 py-2 text-white disabled:opacity-50">
		{saving ? '儲存中…' : '儲存'}
	</button>
</form>
