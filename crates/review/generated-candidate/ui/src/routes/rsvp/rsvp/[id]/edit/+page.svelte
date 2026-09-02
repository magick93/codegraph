<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { RsvpDescriptor } from '../../../descriptor';
	import { apiClient } from '#lib/api/client.ts';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(RsvpDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/rsvp/rsvp/${data.id}`);
	}

	function handleCancel() {
		goto(`/rsvp/rsvp/${data.id}`);
	}
</script>

<EntityForm
	descriptor={RsvpDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
