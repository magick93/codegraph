<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { FormattedDateTimeDescriptor } from '../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	const api = createEntityApi(FormattedDateTimeDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/common/formatted-date-time/${item.id}`);
	}

	function handleCancel() {
		goto('/common/formatted-date-time');
	}
</script>

<EntityForm
	descriptor={FormattedDateTimeDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
