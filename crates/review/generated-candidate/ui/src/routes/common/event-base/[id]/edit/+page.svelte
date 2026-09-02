<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { EventBaseDescriptor } from '../../../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(EventBaseDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/common/event-base/${data.id}`);
	}

	function handleCancel() {
		goto(`/common/event-base/${data.id}`);
	}
</script>

<EntityForm
	descriptor={EventBaseDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
