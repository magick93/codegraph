<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { PersonBaseDescriptor } from '../../../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(PersonBaseDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/common/person-base/${data.id}`);
	}

	function handleCancel() {
		goto(`/common/person-base/${data.id}`);
	}
</script>

<EntityForm
	descriptor={PersonBaseDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
