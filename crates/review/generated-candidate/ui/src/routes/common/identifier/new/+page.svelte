<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { IdentifierDescriptor } from '../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	const api = createEntityApi(IdentifierDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/common/identifier/${item.id}`);
	}

	function handleCancel() {
		goto('/common/identifier');
	}
</script>

<EntityForm
	descriptor={IdentifierDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
