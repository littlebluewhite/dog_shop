<script lang="ts">
	import { toast } from '$lib/toast.svelte';
</script>

<div class="pointer-events-none fixed inset-x-0 bottom-24 z-50 flex flex-col items-center gap-2 px-4 md:bottom-6">
	{#each toast.items as item (item.id)}
		<div class="rounded-full bg-ink px-4 py-2 text-sm font-medium text-ground shadow-[0_8px_24px_rgba(22,39,74,.18)]">{item.message}</div>
	{/each}
</div>
