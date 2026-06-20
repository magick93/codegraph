<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { NameDescriptor } from '../../../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(NameDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/common/name/${data.id}`);
	}

	function handleCancel() {
		goto(`/common/name/${data.id}`);
	}
</script>

<EntityForm
	descriptor={NameDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
