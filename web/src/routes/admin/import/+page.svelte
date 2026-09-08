<script lang="ts">
	import { api, ApiError } from '$lib/api';
	import { canCommit, summarize } from '$lib/importPreview';
	import type { ImportCommit, ImportPreview } from '$lib/types';

	let file = $state<File | null>(null);
	let preview = $state<ImportPreview | null>(null);
	let result = $state<ImportCommit | null>(null);
	let busy = $state<'preview' | 'commit' | null>(null);
	let error = $state('');
	let fieldErrors = $state<Record<string, string>>({});
	/** 兩段確認：第一次按「確認匯入」只顯示確認鈕，第二次才真的送出 */
	let confirming = $state(false);

	function onFile(e: Event) {
		const input = e.currentTarget as HTMLInputElement;
		file = input.files?.[0] ?? null;
		preview = null;
		result = null;
		error = '';
		fieldErrors = {};
		confirming = false;
	}

	async function doPreview() {
		if (!file) return;
		busy = 'preview';
		error = '';
		fieldErrors = {};
		result = null;
		confirming = false;
		try {
			const form = new FormData();
			form.append('file', file);
			preview = await api<ImportPreview>('/api/admin/import/preview', { method: 'POST', body: form });
		} catch (e) {
			preview = null;
			if (e instanceof ApiError) {
				fieldErrors = e.fields();
				error = e.field('file') ?? e.message;
			} else {
				error = '預覽失敗，請再試一次';
			}
		} finally {
			busy = null;
		}
	}

	async function doCommit() {
		if (!file || !preview || !canCommit(preview)) return;
		busy = 'commit';
		error = '';
		fieldErrors = {};
		try {
			const form = new FormData();
			form.append('fingerprint', preview.fingerprint);
			form.append('file', file);
			result = await api<ImportCommit>('/api/admin/import/commit', { method: 'POST', body: form });
			confirming = false;
		} catch (e) {
			confirming = false;
			if (e instanceof ApiError) {
				fieldErrors = e.fields();
				error = e.field('rows') ?? e.field('fingerprint') ?? e.field('file') ?? e.message;
			} else {
				error = '匯入失敗，請再試一次';
			}
		} finally {
			busy = null;
		}
	}

	const summary = $derived(preview ? summarize(preview) : null);
	const shown = $derived(preview ? preview.parsed.products.slice(0, 50) : []);

	const btn = 'rounded px-3 py-2 text-sm disabled:opacity-50';
	const primary = `${btn} bg-gray-900 text-white`;
	const secondary = `${btn} border border-gray-300`;
</script>

<svelte:head><title>匯入商品</title></svelte:head>

<h1 class="text-2xl font-bold">匯入商品</h1>
<p class="mt-1 text-sm text-gray-600">
	上傳 xlsx（依範本或蝦皮匯出檔）。第一列是標題：商品編號、商品名稱、商品描述、分類、規格名稱1、規格選項1、規格名稱2、規格選項2、價格、庫存、SKU、圖片網址（逗號分隔，最多 9 個）。同一個商品編號的多列會合併成多規格；已存在的商品編號會更新既有商品。匯入的新商品是「草稿」，檢查後再上架。
</p>

<div class="mt-4 flex flex-wrap items-center gap-3">
	<input
		type="file"
		accept=".xlsx,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
		onchange={onFile}
		disabled={busy !== null}
	/>
	<button type="button" class={secondary} onclick={doPreview} disabled={!file || busy !== null}>
		{busy === 'preview' ? '解析中…' : '預覽'}
	</button>
</div>

{#if error}
	<p class="mt-3 text-sm text-red-700" role="alert">{error}</p>
{/if}

{#if preview && summary}
	<section class="mt-6">
		<p class="font-medium">{summary.line}</p>
		<p class="text-sm text-gray-600">工作表「{preview.parsed.sheet}」，標題在第 {preview.parsed.header_row} 列，共 {preview.parsed.row_count} 列資料。</p>
		{#if preview.parsed.unmatched_columns.length > 0}
			<p class="mt-2 text-sm text-amber-700">對不上的欄位（會被忽略）：{preview.parsed.unmatched_columns.join('、')}</p>
		{/if}
		{#if preview.parsed.errors.length > 0}
			<h2 class="mt-4 font-medium text-red-700">{preview.parsed.errors.length} 個錯誤，修正後重新上傳才能匯入</h2>
			<div class="mt-2 overflow-x-auto">
				<table class="w-full bg-white text-sm">
					<thead>
						<tr class="border-b border-gray-200 text-left">
							<th class="p-2">列</th>
							<th class="p-2">欄位</th>
							<th class="p-2">問題</th>
						</tr>
					</thead>
					<tbody>
						{#each preview.parsed.errors as e (e.row + (e.column ?? '') + e.message)}
							<tr class="border-b border-gray-100 text-red-700">
								<td class="p-2">{e.row}</td>
								<td class="p-2">{e.column ?? '—'}</td>
								<td class="p-2">{e.message}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
		<h2 class="mt-4 font-medium">商品（前 50 筆）</h2>
		<div class="mt-2 overflow-x-auto">
			<table class="w-full bg-white text-sm">
				<thead>
					<tr class="border-b border-gray-200 text-left">
						<th class="p-2">商品編號</th>
						<th class="p-2">名稱</th>
						<th class="p-2">分類</th>
						<th class="p-2 text-right">規格</th>
						<th class="p-2 text-right">圖片</th>
					</tr>
				</thead>
				<tbody>
					{#each shown as p (p.external_ref)}
						<tr class="border-b border-gray-100">
							<td class="p-2">{p.external_ref}</td>
							<td class="p-2">{p.name}</td>
							<td class="p-2">{p.category ?? '—'}</td>
							<td class="p-2 text-right">{p.variants.length}</td>
							<td class="p-2 text-right">{p.image_urls.length}</td>
						</tr>
					{:else}
						<tr><td colspan="5" class="p-6 text-center text-gray-500">沒有商品</td></tr>
					{/each}
				</tbody>
			</table>
		</div>
		<div class="mt-4">
			{#if confirming}
				<span class="flex flex-wrap items-center gap-2">
					<button type="button" class={primary} onclick={doCommit} disabled={busy !== null}>
						{busy === 'commit' ? '匯入中，請不要關閉頁面…' : `再按一次確認匯入 ${preview.product_count} 個商品`}
					</button>
					<button type="button" class={secondary} onclick={() => (confirming = false)} disabled={busy !== null}>返回</button>
				</span>
			{:else}
				<button type="button" class={primary} onclick={() => (confirming = true)} disabled={!canCommit(preview) || busy !== null}>
					確認匯入
				</button>
			{/if}
		</div>
	</section>
{/if}

{#if result}
	<section class="mt-6">
		<h2 class="font-medium">匯入完成：新增 {result.result.created}、更新 {result.result.updated}</h2>
		{#if result.result.warnings.length > 0}
			<h3 class="mt-3 text-amber-700">{result.result.warnings.length} 個警告（商品已匯入，只是圖片沒抓到）</h3>
			<ul class="mt-1 list-disc pl-5 text-sm">
				{#each result.result.warnings as w (w.external_ref + w.row + w.message)}
					<li>{w.external_ref}（第 {w.row} 列）：{w.message}</li>
				{/each}
			</ul>
		{/if}
		<ul class="mt-3 text-sm">
			{#each result.result.products as p (p.id)}
				<li><a class="underline" href={`/admin/products/${p.id}`}>{p.name}</a>（{p.created ? '新增' : '更新'}，{p.images} 張圖）</li>
			{/each}
		</ul>
	</section>
{/if}
