<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { NameDescriptor } from '../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	const api = createEntityApi(NameDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/common/name/${item.id}`);
	}

	function handleCancel() {
		goto('/common/name');
	}
</script>

<EntityForm
	descriptor={NameDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
