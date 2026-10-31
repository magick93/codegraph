<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { EventBaseDescriptor } from '../descriptor';
	import { apiClient } from '#lib/api/client.ts';
	import { goto } from '$app/navigation';

	const api = createEntityApi(EventBaseDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/common/event-base/${item.id}`);
	}

	function handleCancel() {
		goto('/common/event-base');
	}
</script>

<EntityForm
	descriptor={EventBaseDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
