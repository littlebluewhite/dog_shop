<script lang="ts">
	import { api, ApiError } from '$lib/api';
	import Alert from '$lib/components/ui/Alert.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import { canCommit, summarize } from '$lib/importPreview';
	import type { ImportCommit, ImportPreview } from '$lib/types';

	let file = $state<File | null>(null);
	let preview = $state<ImportPreview | null>(null);
	let result = $state<ImportCommit | null>(null);
	let busy = $state<'preview' | 'commit' | null>(null);
	let error = $state('');
	/** 兩段確認：第一次按「確認匯入」只顯示確認鈕，第二次才真的送出 */
	let confirming = $state(false);

	function onFile(e: Event) {
		const input = e.currentTarget as HTMLInputElement;
		file = input.files?.[0] ?? null;
		preview = null;
		result = null;
		error = '';
		confirming = false;
	}

	async function doPreview() {
		if (!file) return;
		busy = 'preview';
		error = '';
		result = null;
		confirming = false;
		try {
			const form = new FormData();
			form.append('file', file);
			preview = await api<ImportPreview>('/api/admin/import/preview', { method: 'POST', body: form });
		} catch (e) {
			preview = null;
			if (e instanceof ApiError) {
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
		// 上一次的「匯入完成」不能留在畫面上跟這次的錯誤並排
		result = null;
		try {
			const form = new FormData();
			form.append('fingerprint', preview.fingerprint);
			form.append('file', file);
			result = await api<ImportCommit>('/api/admin/import/commit', { method: 'POST', body: form });
			// 收掉預覽（含「確認匯入」鈕）：同一份檔案再按一次會整批重跑、圖片全部重抓
			preview = null;
			confirming = false;
		} catch (e) {
			confirming = false;
			if (e instanceof ApiError) {
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
</script>

<svelte:head><title>匯入商品</title></svelte:head>

<PageHeader title="匯入商品" />
<p class="mt-2 max-w-[65ch] text-sm text-ink-soft">
	上傳 xlsx（依範本或蝦皮匯出檔）。第一列是標題：商品編號、商品名稱、商品描述、分類、規格名稱1、規格選項1、規格名稱2、規格選項2、價格、庫存、SKU、圖片網址（逗號分隔，最多 9 個）。同一個商品編號的多列會合併成多規格；已存在的商品編號會更新既有商品。匯入的新商品是「草稿」，檢查後再上架。<strong class="text-ink">工作表沒有列到的規格會被刪除（有訂單引用的會改成停用），所以要改價也必須把該商品全部的規格都列出來，不能只列要改的那幾個。</strong>
</p>
<div class="mt-4 flex flex-wrap items-center gap-3">
	<input
		type="file"
		accept=".xlsx,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
		onchange={onFile}
		disabled={busy !== null}
		class="text-sm file:mr-3 file:rounded-full file:border file:border-line file:bg-ground file:px-4 file:py-2 file:text-sm file:font-semibold file:text-ink"
	/>
	<Button variant="secondary" onclick={doPreview} disabled={!file || busy !== null}>
		{busy === 'preview' ? '解析中…' : '預覽'}
	</Button>
</div>
{#if error}
	<Alert tone="danger" class="mt-3">{error}</Alert>
{/if}

{#if preview && summary}
	<Card class="mt-6">
		<p class="font-semibold">{summary.line}</p>
		<p class="text-sm text-ink-soft">工作表「{preview.parsed.sheet}」，標題在第 {preview.parsed.header_row} 列，共 {preview.parsed.row_count} 列資料。</p>
		{#if preview.parsed.unmatched_columns.length > 0}
			<p class="mt-2 text-sm text-warning">對不上的欄位（會被忽略）：{preview.parsed.unmatched_columns.join('、')}</p>
		{/if}
		{#if preview.parsed.errors.length > 0}
			<h3 class="mt-4 font-semibold text-danger">{preview.parsed.errors.length} 個錯誤，修正後重新上傳才能匯入</h3>
			<div class="mt-2 overflow-x-auto">
				<table class="table">
					<thead>
						<tr>
							<th>列</th>
							<th>欄位</th>
							<th>問題</th>
						</tr>
					</thead>
					<tbody>
						{#each preview.parsed.errors as e (e.row + (e.column ?? '') + e.message)}
							<tr class="text-danger">
								<td class="tabular-nums">{e.row}</td>
								<td>{e.column ?? '—'}</td>
								<td>{e.message}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
		<h3 class="mt-4 font-semibold">商品（前 50 筆）</h3>
		<div class="mt-2 overflow-x-auto">
			<table class="table">
				<thead>
					<tr>
						<th>商品編號</th>
						<th>名稱</th>
						<th>分類</th>
						<th class="text-right">規格</th>
						<th class="text-right">圖片</th>
					</tr>
				</thead>
				<tbody>
					{#each shown as p (p.external_ref)}
						<tr>
							<td>{p.external_ref}</td>
							<td>{p.name}</td>
							<td>{p.category ?? '—'}</td>
							<td class="text-right tabular-nums">{p.variants.length}</td>
							<td class="text-right tabular-nums">{p.image_urls.length}</td>
						</tr>
					{:else}
						<tr><td colspan="5" class="p-6 text-center text-ink-soft">沒有商品</td></tr>
					{/each}
				</tbody>
			</table>
		</div>
		<div class="mt-4">
			{#if confirming}
				<span class="flex flex-wrap items-center gap-2">
					<Button onclick={doCommit} disabled={busy !== null}>
						{busy === 'commit' ? '匯入中，請不要關閉頁面…' : `再按一次確認匯入 ${preview.product_count} 個商品`}
					</Button>
					<Button variant="secondary" onclick={() => (confirming = false)} disabled={busy !== null}>返回</Button>
				</span>
			{:else}
				<Button onclick={() => (confirming = true)} disabled={!canCommit(preview) || busy !== null}>確認匯入</Button>
			{/if}
		</div>
	</Card>
{/if}

{#if result}
	<Card class="mt-6" title={`匯入完成：新增 ${result.result.created}、更新 ${result.result.updated}`}>
		{#if result.result.warnings.length > 0}
			<h3 class="text-warning">{result.result.warnings.length} 個警告（商品已匯入，只是圖片沒抓到）</h3>
			<ul class="mt-1 list-disc pl-5 text-sm">
				{#each result.result.warnings as w (w.external_ref + w.row + w.message)}
					<li>{w.external_ref}（第 {w.row} 列）：{w.message}</li>
				{/each}
			</ul>
		{/if}
		<ul class="mt-3 text-sm">
			{#each result.result.products as p (p.id)}
				<li><a class="link" href={`/admin/products/${p.id}`}>{p.name}</a>（{p.created ? '新增' : '更新'}，{p.images} 張圖）</li>
			{/each}
		</ul>
	</Card>
{/if}
