<script lang="ts">
	import { invalidateAll } from '$app/navigation';
	import { api, ApiError } from '$lib/api';
	import AddressFields from '$lib/components/AddressFields.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Field from '$lib/components/ui/Field.svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
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

<PageHeader title="常用地址">
	{#if editing === null && data.addresses.length < 10}
		<Button onclick={startNew}>新增地址</Button>
	{/if}
</PageHeader>

{#if editing !== null}
	<form onsubmit={save} class="mt-4 max-w-lg" novalidate>
		<Card>
			<div class="space-y-3">
				<div class="grid grid-cols-2 gap-3">
					<Field label="收件人" type="text" bind:value={form.recipient_name} error={errors.recipient_name} />
					<Field label="手機" type="tel" bind:value={form.phone} placeholder="09xxxxxxxx" error={errors.phone} />
				</div>
				<AddressFields bind:address={addressPart} {errors} />
				<label class="flex items-center gap-2 text-sm">
					<input type="checkbox" bind:checked={form.is_default} class="accent-brand" /> 設為預設地址
				</label>
				<div class="flex gap-2">
					<Button type="submit" disabled={saving}>{saving ? '儲存中…' : '儲存'}</Button>
					<Button variant="secondary" onclick={cancel}>取消</Button>
				</div>
			</div>
		</Card>
	</form>
{/if}

{#if data.addresses.length === 0 && editing === null}
	<p class="mt-4 text-ink-soft">還沒有常用地址。</p>
{:else}
	<ul class="mt-4 space-y-3">
		{#each data.addresses as a (a.id)}
			<li>
				<Card>
					<div class="flex flex-wrap items-center gap-3">
						<div class="min-w-0 flex-1 text-sm">
							<div class="flex items-center gap-2 font-semibold">
								{a.recipient_name}
								{#if a.is_default}<Badge tone="brand">預設</Badge>{/if}
							</div>
							<div class="text-ink-soft">{a.phone}</div>
							<div class="text-ink-soft">{a.postal_code} {a.city}{a.district}{a.street}</div>
						</div>
						<Button variant="ghost" size="sm" onclick={() => startEdit(a)}>編輯</Button>
						{#if confirmDeleteId === a.id}
							<Button variant="danger" size="sm" onclick={() => remove(a.id)}>確定刪除？</Button>
						{:else}
							<Button variant="ghost" size="sm" onclick={() => (confirmDeleteId = a.id)}>刪除</Button>
						{/if}
					</div>
				</Card>
			</li>
		{/each}
	</ul>
{/if}
