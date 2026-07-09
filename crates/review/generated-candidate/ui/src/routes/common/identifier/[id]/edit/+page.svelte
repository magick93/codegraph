<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { IdentifierDescriptor } from '../../../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(IdentifierDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/common/identifier/${data.id}`);
	}

	function handleCancel() {
		goto(`/common/identifier/${data.id}`);
	}
</script>

<EntityForm
	descriptor={IdentifierDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
