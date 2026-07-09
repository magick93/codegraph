<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { CodeDescriptor } from '../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	const api = createEntityApi(CodeDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/common/code/${item.id}`);
	}

	function handleCancel() {
		goto('/common/code');
	}
</script>

<EntityForm
	descriptor={CodeDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
