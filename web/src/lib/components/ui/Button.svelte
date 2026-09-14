<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { HTMLAttributes } from 'svelte/elements';

	type Variant = 'primary' | 'secondary' | 'ghost' | 'danger';
	type Size = 'sm' | 'md' | 'lg';

	// rest 用 HTMLAttributes<HTMLElement>（不是 HTMLButtonAttributes）：同一份 rest 會展開到 <a>、<span>、<button> 三種元素，
	// 事件型別才不會互相打架
	let {
		variant = 'primary',
		size = 'md',
		href,
		disabled = false,
		type = 'button',
		class: className = '',
		children,
		...rest
	}: {
		variant?: Variant;
		size?: Size;
		href?: string;
		disabled?: boolean;
		type?: 'button' | 'submit' | 'reset';
		class?: string;
		children: Snippet;
	} & Omit<HTMLAttributes<HTMLElement>, 'class'> = $props();

	const VARIANTS: Record<Variant, string> = {
		primary: 'bg-brand text-ink hover:bg-brand-deep',
		secondary: 'border border-line bg-ground text-ink hover:bg-surface',
		ghost: 'text-ink hover:bg-surface',
		danger: 'bg-danger text-ground hover:bg-danger/90'
	};
	const SIZES: Record<Size, string> = {
		sm: 'h-9 px-3.5 text-sm',
		md: 'h-11 px-5 text-base',
		lg: 'h-12 px-6 text-base'
	};
	const classes = $derived(
		`inline-flex items-center justify-center gap-2 rounded-full font-semibold whitespace-nowrap transition-colors duration-150 active:scale-[.98] disabled:pointer-events-none disabled:opacity-50 aria-disabled:pointer-events-none aria-disabled:opacity-50 ${VARIANTS[variant]} ${SIZES[size]} ${className}`
	);
</script>

{#if href && !disabled}
	<a {href} class={classes} {...rest}>{@render children()}</a>
{:else if href}
	<span class={classes} aria-disabled="true" {...rest}>{@render children()}</span>
{:else}
	<button {type} {disabled} class={classes} {...rest}>{@render children()}</button>
{/if}
