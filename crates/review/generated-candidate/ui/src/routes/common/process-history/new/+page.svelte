<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { ProcessHistoryDescriptor } from '../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	const api = createEntityApi(ProcessHistoryDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/common/process-history/${item.id}`);
	}

	function handleCancel() {
		goto('/common/process-history');
	}
</script>

<EntityForm
	descriptor={ProcessHistoryDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
