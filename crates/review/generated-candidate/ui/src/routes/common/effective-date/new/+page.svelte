<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { EffectiveDateDescriptor } from '../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	const api = createEntityApi(EffectiveDateDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/common/effective-date/${item.id}`);
	}

	function handleCancel() {
		goto('/common/effective-date');
	}
</script>

<EntityForm
	descriptor={EffectiveDateDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
