<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { PublicEventDescriptor } from '../../../descriptor';
	import { apiClient } from '#lib/api/client.ts';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(PublicEventDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/events/public-event/${data.id}`);
	}

	function handleCancel() {
		goto(`/events/public-event/${data.id}`);
	}
</script>

<EntityForm
	descriptor={PublicEventDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
