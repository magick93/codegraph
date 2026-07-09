<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { AmountDescriptor } from '../../../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(AmountDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/common/amount/${data.id}`);
	}

	function handleCancel() {
		goto(`/common/amount/${data.id}`);
	}
</script>

<EntityForm
	descriptor={AmountDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
