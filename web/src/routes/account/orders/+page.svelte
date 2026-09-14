<script lang="ts">
	import Pagination from '$lib/components/Pagination.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import StatusBadge from '$lib/components/ui/StatusBadge.svelte';
	import { formatDate, twd } from '$lib/format';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();
</script>

<svelte:head><title>我的訂單</title></svelte:head>

<PageHeader title="我的訂單" />

{#if data.result.items.length === 0}
	<div class="mt-4">
		<EmptyState message="還沒有訂單。"><Button variant="secondary" href="/products">去逛逛</Button></EmptyState>
	</div>
{:else}
	<div class="card mt-4 overflow-x-auto p-0 md:p-0">
		<table class="table">
			<thead>
				<tr><th>訂單編號</th><th>日期</th><th>狀態</th><th>件數</th><th class="text-right">金額</th></tr>
			</thead>
			<tbody>
				{#each data.result.items as o (o.id)}
					<tr>
						<td><a href={`/orders/${o.id}`} class="link">{o.order_no}</a></td>
						<td class="text-ink-soft">{formatDate(o.created_at)}</td>
						<td><StatusBadge status={o.status} /></td>
						<td class="tabular-nums">{o.item_count}</td>
						<td class="text-right tabular-nums">{twd(o.total)}</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
	<Pagination page={data.result.page} perPage={data.result.per_page} total={data.result.total} />
{/if}
