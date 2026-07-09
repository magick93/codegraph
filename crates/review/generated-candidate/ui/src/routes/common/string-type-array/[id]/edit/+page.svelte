<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { StringTypeArrayDescriptor } from '../../../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(StringTypeArrayDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/common/string-type-array/${data.id}`);
	}

	function handleCancel() {
		goto(`/common/string-type-array/${data.id}`);
	}
</script>

<EntityForm
	descriptor={StringTypeArrayDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
