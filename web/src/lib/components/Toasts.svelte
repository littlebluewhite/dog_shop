<script lang="ts">
	import { toast } from '$lib/toast.svelte';
</script>

<div class="pointer-events-none fixed inset-x-0 bottom-6 z-50 flex flex-col items-center gap-2">
	{#each toast.items as item (item.id)}
		<div class="rounded-full bg-gray-900 px-4 py-2 text-sm text-white shadow-lg">{item.message}</div>
	{/each}
</div>
