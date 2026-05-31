<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { ApplicationDescriptor } from '../../../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(ApplicationDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/recruiting/applications/${data.id}`);
	}

	function handleCancel() {
		goto(`/recruiting/applications/${data.id}`);
	}
</script>

<EntityForm
	descriptor={ApplicationDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
