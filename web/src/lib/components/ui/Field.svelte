<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLInputAttributes } from 'svelte/elements';

	// value 刻意用 any：bind:value 是雙向的，父層可能綁 string、number、null；用 unknown 或 string | number 都會讓其中一個方向型別不合
	let {
		label,
		value = $bindable(),
		error,
		hint,
		class: className = '',
		children,
		...rest
	}: {
		label: string;
		// eslint-disable-next-line @typescript-eslint/no-explicit-any
		value?: any;
		error?: string;
		hint?: string;
		class?: string;
		children?: Snippet;
	} & Omit<HTMLInputAttributes, 'value' | 'class'> = $props();

	const uid = $props.id();
	const errorId = $derived(error ? `${uid}-error` : undefined);
</script>

<!-- label 包住控制項（Playwright getByLabel 靠這個）；hint / error 放 label 外面，label 的可讀名稱才只有標籤字 -->
<div class={className}>
	<label class="block">
		<span class="field-label">{label}</span>
		{#if children}
			{@render children()}
		{:else}
			<input class="input mt-1" bind:value aria-invalid={error ? 'true' : undefined} aria-describedby={errorId} {...rest} />
		{/if}
	</label>
	{#if hint}<p class="field-hint">{hint}</p>{/if}
	{#if error}<p id={errorId} class="field-error">{error}</p>{/if}
</div>
