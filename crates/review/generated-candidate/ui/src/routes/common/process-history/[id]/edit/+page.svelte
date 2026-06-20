<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { ProcessHistoryDescriptor } from '../../../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(ProcessHistoryDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/common/process-history/${data.id}`);
	}

	function handleCancel() {
		goto(`/common/process-history/${data.id}`);
	}
</script>

<EntityForm
	descriptor={ProcessHistoryDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
