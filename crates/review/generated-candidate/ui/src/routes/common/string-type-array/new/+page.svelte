<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { StringTypeArrayDescriptor } from '../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	const api = createEntityApi(StringTypeArrayDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/common/string-type-array/${item.id}`);
	}

	function handleCancel() {
		goto('/common/string-type-array');
	}
</script>

<EntityForm
	descriptor={StringTypeArrayDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
