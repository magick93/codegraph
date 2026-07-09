<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { CurrencyCodeListDescriptor } from '../../../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(CurrencyCodeListDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/common/currency-code-list/${data.id}`);
	}

	function handleCancel() {
		goto(`/common/currency-code-list/${data.id}`);
	}
</script>

<EntityForm
	descriptor={CurrencyCodeListDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
