<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { ApplicationDescriptor } from '../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	const api = createEntityApi(ApplicationDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/recruiting/applications/${item.id}`);
	}

	function handleCancel() {
		goto('/recruiting/applications');
	}
</script>

<EntityForm
	descriptor={ApplicationDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
