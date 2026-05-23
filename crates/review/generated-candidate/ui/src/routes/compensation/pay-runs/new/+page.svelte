<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { PayRunDescriptor } from '../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	const api = createEntityApi(PayRunDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/compensation/pay-runs/${item.id}`);
	}

	function handleCancel() {
		goto('/compensation/pay-runs');
	}
</script>

<EntityForm
	descriptor={PayRunDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
