<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { CurrencyCodeListDescriptor } from '../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	const api = createEntityApi(CurrencyCodeListDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/common/currency-code-list/${item.id}`);
	}

	function handleCancel() {
		goto('/common/currency-code-list');
	}
</script>

<EntityForm
	descriptor={CurrencyCodeListDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
