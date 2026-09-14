<script lang="ts">
	import { untrack } from 'svelte';
	import { invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import { toast } from '$lib/toast.svelte';
	import type { Category } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	let newName = $state('');
	let newSlug = $state('');
	let confirmDeleteId = $state<string | null>(null);

	// 每列可編輯的複本；伺服器資料重新載入時同步回來（untrack：初始值刻意只取一次）
	let rows = $state<Category[]>(untrack(() => data.categories.map((c) => ({ ...c }))));
	$effect(() => {
		rows = data.categories.map((c) => ({ ...c }));
	});

	function showError(err: unknown, fallback: string) {
		if (err instanceof ApiError) toast.show(err.field('slug') ?? err.field('name') ?? err.message);
		else toast.show(fallback);
	}

	async function create(e: SubmitEvent) {
		e.preventDefault();
		try {
			await api('/api/admin/categories', {
				method: 'POST',
				body: JSON.stringify({ name: newName, slug: newSlug.trim() || null, sort_order: rows.length })
			});
			newName = '';
			newSlug = '';
			toast.show('已新增分類');
			await invalidateAll();
		} catch (err) {
			showError(err, '新增失敗');
		}
	}

	async function save(row: Category) {
		try {
			await api(`/api/admin/categories/${row.id}`, {
				method: 'PUT',
				body: JSON.stringify({ name: row.name, slug: row.slug, sort_order: Number(row.sort_order) || 0 })
			});
			toast.show('已儲存');
			await invalidateAll();
		} catch (err) {
			showError(err, '儲存失敗');
		}
	}

	async function remove(id: string) {
		// 第一次按只變成「確定刪除？」，第二次才真的刪（不用 confirm() 對話框）
		if (confirmDeleteId !== id) {
			confirmDeleteId = id;
			return;
		}
		try {
			await api(`/api/admin/categories/${id}`, { method: 'DELETE' });
			toast.show('已刪除');
			confirmDeleteId = null;
			await invalidateAll();
		} catch (err) {
			showError(err, '刪除失敗');
		}
	}
</script>

<svelte:head><title>分類管理</title></svelte:head>

<PageHeader title="分類" />

<form onsubmit={create} class="mt-5">
	<Card>
		<div class="flex flex-wrap items-end gap-2">
			<Field label="名稱" bind:value={newName} required class="w-48" />
			<Field label="網址代稱（可空白，會自動產生）" bind:value={newSlug} placeholder="例如 food" class="w-64" />
			<Button type="submit">新增</Button>
		</div>
	</Card>
</form>

<div class="card mt-6 overflow-x-auto p-0 md:p-0">
	<table class="table">
		<thead>
			<tr>
				<th>排序</th>
				<th>名稱</th>
				<th>網址代稱</th>
				<th></th>
			</tr>
		</thead>
		<tbody>
			{#each rows as row (row.id)}
				<tr>
					<td><input type="number" bind:value={row.sort_order} class="input w-20 py-1.5" /></td>
					<td><input bind:value={row.name} class="input py-1.5" /></td>
					<td><input bind:value={row.slug} class="input py-1.5" /></td>
					<td class="whitespace-nowrap">
						<Button variant="secondary" size="sm" onclick={() => save(row)}>儲存</Button>
						<Button variant={confirmDeleteId === row.id ? 'danger' : 'secondary'} size="sm" class="ml-2" onclick={() => remove(row.id)}>
							{confirmDeleteId === row.id ? '確定刪除？' : '刪除'}
						</Button>
					</td>
				</tr>
			{:else}
				<tr><td colspan="4" class="p-4 text-center text-ink-soft">還沒有分類</td></tr>
			{/each}
		</tbody>
	</table>
</div>
