<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { DistributionBaseDescriptor } from '../../../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(DistributionBaseDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/common/distribution-base/${data.id}`);
	}

	function handleCancel() {
		goto(`/common/distribution-base/${data.id}`);
	}
</script>

<EntityForm
	descriptor={DistributionBaseDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
