<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { RsvpDescriptor } from '../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	const api = createEntityApi(RsvpDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/rsvp/rsvp/${item.id}`);
	}

	function handleCancel() {
		goto('/rsvp/rsvp');
	}
</script>

<EntityForm
	descriptor={RsvpDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
