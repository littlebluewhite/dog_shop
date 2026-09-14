<script lang="ts">
	import { ORDER_STATUS_LABELS } from '$lib/labels';
	import type { OrderStatus } from '$lib/types';
	import Badge from './Badge.svelte';
	import { orderStatusTone } from './status';

	let { status }: { status: OrderStatus } = $props();
</script>

<Badge tone={orderStatusTone(status)}>{ORDER_STATUS_LABELS[status]}</Badge>
