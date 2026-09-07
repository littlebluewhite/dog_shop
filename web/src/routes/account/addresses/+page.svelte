<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import AddressFields from '$lib/components/AddressFields.svelte';
	import { toast } from '$lib/toast.svelte';
	import type { Address, AddressInput, HomeAddressInput } from '$lib/types';
	import type { PageProps } from './$types';

	let { data }: PageProps = $props();

	function blank(): AddressInput {
		return { recipient_name: '', phone: '', postal_code: '', city: '', district: '', street: '', is_default: false };
	}

	let editing = $state<string | 'new' | null>(null);
	let form = $state<AddressInput>(blank());
	let errors = $state<Record<string, string>>({});
	let saving = $state(false);
	let confirmDeleteId = $state<string | null>(null);

	// AddressFields 只管地址四欄；用一個 derived-like 物件把它綁回 form
	let addressPart = $state<HomeAddressInput>({ postal_code: '', city: '', district: '', street: '' });
	$effect(() => {
		form.postal_code = addressPart.postal_code;
		form.city = addressPart.city;
		form.district = addressPart.district;
		form.street = addressPart.street;
	});

	function startNew() {
		editing = 'new';
		form = blank();
		addressPart = { postal_code: '', city: '', district: '', street: '' };
		errors = {};
	}
	function startEdit(a: Address) {
		editing = a.id;
		form = { ...a };
		addressPart = { postal_code: a.postal_code, city: a.city, district: a.district, street: a.street };
		errors = {};
	}
	function cancel() {
		editing = null;
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		errors = {};
		saving = true;
		try {
			if (editing === 'new') {
				await api<Address>('/api/me/addresses', { method: 'POST', body: JSON.stringify(form) });
			} else {
				await api<Address>(`/api/me/addresses/${editing}`, { method: 'PUT', body: JSON.stringify(form) });
			}
			editing = null;
			await invalidateAll();
			toast.show('已儲存');
		} catch (err) {
			if (err instanceof ApiError) {
				errors = err.fields();
				if (Object.keys(errors).length === 0) toast.show(err.message);
			} else toast.show('儲存失敗');
		} finally {
			saving = false;
		}
	}

	async function remove(id: string) {
		try {
			await api(`/api/me/addresses/${id}`, { method: 'DELETE' });
			confirmDeleteId = null;
			await invalidateAll();
			toast.show('已刪除');
		} catch {
			toast.show('刪除失敗');
		}
	}
</script>

<svelte:head><title>常用地址</title></svelte:head>

<div class="flex items-center justify-between">
	<h1 class="text-2xl font-bold">常用地址</h1>
	{#if editing === null && data.addresses.length < 10}
		<button type="button" onclick={startNew} class="rounded bg-gray-900 px-4 py-2 text-sm text-white">新增地址</button>
	{/if}
</div>

{#if editing !== null}
	<form onsubmit={save} class="mt-4 max-w-lg space-y-3 rounded border border-gray-200 bg-white p-4" novalidate>
		<div class="grid grid-cols-2 gap-3">
			<label class="block text-sm text-gray-700">
				收件人
				<input type="text" bind:value={form.recipient_name} class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
				{#if errors.recipient_name}<span class="text-red-600">{errors.recipient_name}</span>{/if}
			</label>
			<label class="block text-sm text-gray-700">
				手機
				<input type="tel" bind:value={form.phone} placeholder="09xxxxxxxx" class="mt-1 w-full rounded border border-gray-300 px-3 py-2" />
				{#if errors.phone}<span class="text-red-600">{errors.phone}</span>{/if}
			</label>
		</div>
		<AddressFields bind:address={addressPart} {errors} />
		<label class="flex items-center gap-2 text-sm text-gray-700">
			<input type="checkbox" bind:checked={form.is_default} /> 設為預設地址
		</label>
		<div class="flex gap-2">
			<button type="submit" disabled={saving} class="rounded bg-gray-900 px-4 py-2 text-white disabled:opacity-50">{saving ? '儲存中…' : '儲存'}</button>
			<button type="button" onclick={cancel} class="rounded border border-gray-300 px-4 py-2">取消</button>
		</div>
	</form>
{/if}

{#if data.addresses.length === 0 && editing === null}
	<p class="mt-4 text-gray-600">還沒有常用地址。</p>
{:else}
	<ul class="mt-4 divide-y divide-gray-200 rounded border border-gray-200 bg-white">
		{#each data.addresses as a (a.id)}
			<li class="flex flex-wrap items-center gap-3 p-4">
				<div class="min-w-0 flex-1 text-sm">
					<div class="font-medium">
						{a.recipient_name}
						{#if a.is_default}<span class="ml-2 rounded bg-gray-900 px-2 py-0.5 text-xs text-white">預設</span>{/if}
					</div>
					<div class="text-gray-600">{a.phone}</div>
					<div class="text-gray-600">{a.postal_code} {a.city}{a.district}{a.street}</div>
				</div>
				<button type="button" onclick={() => startEdit(a)} class="text-sm underline">編輯</button>
				{#if confirmDeleteId === a.id}
					<button type="button" onclick={() => remove(a.id)} class="text-sm text-red-600 underline">確定刪除？</button>
				{:else}
					<button type="button" onclick={() => (confirmDeleteId = a.id)} class="text-sm text-gray-500 underline">刪除</button>
				{/if}
			</li>
		{/each}
	</ul>
{/if}
