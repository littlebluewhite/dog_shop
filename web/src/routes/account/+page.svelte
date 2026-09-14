<script lang="ts">
	import { untrack } from 'svelte';
	import { invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
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

<PageHeader title="個人資料" subtitle={data.user?.email} />

<form onsubmit={save} class="mt-6 max-w-md space-y-4" novalidate>
	<Card>
		<div class="space-y-4">
			<Field label="姓名" type="text" bind:value={name} error={errors.name} />
			<Field label="手機" type="tel" bind:value={phone} placeholder="09xxxxxxxx" error={errors.phone} />
		</div>
	</Card>
	<Card title="更改密碼（不改就留空）">
		<div class="space-y-4">
			<Field label="目前密碼" type="password" bind:value={currentPassword} autocomplete="current-password" error={errors.current_password} />
			<Field label="新密碼（至少 8 碼）" type="password" bind:value={newPassword} autocomplete="new-password" error={errors.new_password} />
		</div>
	</Card>
	<Button type="submit" disabled={saving}>{saving ? '儲存中…' : '儲存'}</Button>
</form>
