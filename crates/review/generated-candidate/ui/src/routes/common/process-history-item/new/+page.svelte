<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { ProcessHistoryItemDescriptor } from '../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	const api = createEntityApi(ProcessHistoryItemDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/common/process-history-item/${item.id}`);
	}

	function handleCancel() {
		goto('/common/process-history-item');
	}
</script>

<EntityForm
	descriptor={ProcessHistoryItemDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
