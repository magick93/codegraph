<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { FormattedDateTimeDescriptor } from '../../../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(FormattedDateTimeDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/common/formatted-date-time/${data.id}`);
	}

	function handleCancel() {
		goto(`/common/formatted-date-time/${data.id}`);
	}
</script>

<EntityForm
	descriptor={FormattedDateTimeDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
