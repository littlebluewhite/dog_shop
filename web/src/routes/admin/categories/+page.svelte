<script lang="ts">
	import { untrack } from 'svelte';
	import { invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
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

<h1 class="text-2xl font-bold">分類</h1>

<form onsubmit={create} class="mt-4 flex flex-wrap items-end gap-2 rounded border border-gray-200 bg-white p-4">
	<label class="block text-sm">
		名稱
		<input bind:value={newName} required class="mt-1 block rounded border border-gray-300 px-3 py-2" />
	</label>
	<label class="block text-sm">
		網址代稱（可空白，會自動產生）
		<input bind:value={newSlug} placeholder="例如 food" class="mt-1 block rounded border border-gray-300 px-3 py-2" />
	</label>
	<button type="submit" class="rounded bg-gray-900 px-4 py-2 text-sm text-white">新增</button>
</form>

<div class="mt-6 overflow-x-auto">
	<table class="w-full border-collapse bg-white text-sm">
		<thead>
			<tr class="border-b border-gray-200 text-left">
				<th class="p-2">排序</th>
				<th class="p-2">名稱</th>
				<th class="p-2">網址代稱</th>
				<th class="p-2"></th>
			</tr>
		</thead>
		<tbody>
			{#each rows as row (row.id)}
				<tr class="border-b border-gray-100">
					<td class="p-2"><input type="number" bind:value={row.sort_order} class="w-16 rounded border border-gray-300 px-2 py-1" /></td>
					<td class="p-2"><input bind:value={row.name} class="w-full rounded border border-gray-300 px-2 py-1" /></td>
					<td class="p-2"><input bind:value={row.slug} class="w-full rounded border border-gray-300 px-2 py-1" /></td>
					<td class="p-2 whitespace-nowrap">
						<button type="button" onclick={() => save(row)} class="rounded border border-gray-300 px-3 py-1">儲存</button>
						<button type="button" onclick={() => remove(row.id)} class="ml-2 rounded border border-red-300 px-3 py-1 text-red-700">
							{confirmDeleteId === row.id ? '確定刪除？' : '刪除'}
						</button>
					</td>
				</tr>
			{:else}
				<tr><td colspan="4" class="p-4 text-center text-gray-500">還沒有分類</td></tr>
			{/each}
		</tbody>
	</table>
</div>
