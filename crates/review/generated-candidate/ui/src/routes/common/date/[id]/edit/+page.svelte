<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { DateDescriptor } from '../../../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(DateDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/common/date/${data.id}`);
	}

	function handleCancel() {
		goto(`/common/date/${data.id}`);
	}
</script>

<EntityForm
	descriptor={DateDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
