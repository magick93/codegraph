<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { DistributionBaseDescriptor } from '../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	const api = createEntityApi(DistributionBaseDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/common/distribution-base/${item.id}`);
	}

	function handleCancel() {
		goto('/common/distribution-base');
	}
</script>

<EntityForm
	descriptor={DistributionBaseDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
