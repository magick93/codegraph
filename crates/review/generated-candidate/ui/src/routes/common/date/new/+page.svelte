<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { DateDescriptor } from '../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	const api = createEntityApi(DateDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/common/date/${item.id}`);
	}

	function handleCancel() {
		goto('/common/date');
	}
</script>

<EntityForm
	descriptor={DateDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
