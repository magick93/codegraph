<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { PublicEventDescriptor } from '../descriptor';
	import { apiClient } from '#lib/api/client.ts';
	import { goto } from '$app/navigation';

	const api = createEntityApi(PublicEventDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/events/public-event/${item.id}`);
	}

	function handleCancel() {
		goto('/events/public-event');
	}
</script>

<EntityForm
	descriptor={PublicEventDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
