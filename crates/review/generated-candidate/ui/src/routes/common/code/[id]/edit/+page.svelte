<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { CodeDescriptor } from '../../../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(CodeDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/common/code/${data.id}`);
	}

	function handleCancel() {
		goto(`/common/code/${data.id}`);
	}
</script>

<EntityForm
	descriptor={CodeDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
