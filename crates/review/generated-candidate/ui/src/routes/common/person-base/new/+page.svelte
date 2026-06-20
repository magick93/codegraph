<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { PersonBaseDescriptor } from '../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	const api = createEntityApi(PersonBaseDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/common/person-base/${item.id}`);
	}

	function handleCancel() {
		goto('/common/person-base');
	}
</script>

<EntityForm
	descriptor={PersonBaseDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
