<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { CandidateDescriptor } from '../../../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(CandidateDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/recruiting/candidate/${data.id}`);
	}

	function handleCancel() {
		goto(`/recruiting/candidate/${data.id}`);
	}
</script>

<EntityForm
	descriptor={CandidateDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
