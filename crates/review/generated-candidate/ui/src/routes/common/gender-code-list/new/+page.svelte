<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { GenderCodeListDescriptor } from '../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	const api = createEntityApi(GenderCodeListDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/common/gender-code-list/${item.id}`);
	}

	function handleCancel() {
		goto('/common/gender-code-list');
	}
</script>

<EntityForm
	descriptor={GenderCodeListDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
