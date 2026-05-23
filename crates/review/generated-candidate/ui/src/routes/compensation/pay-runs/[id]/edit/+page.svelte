<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { PayRunDescriptor } from '../../../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(PayRunDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/compensation/pay-runs/${data.id}`);
	}

	function handleCancel() {
		goto(`/compensation/pay-runs/${data.id}`);
	}
</script>

<EntityForm
	descriptor={PayRunDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
