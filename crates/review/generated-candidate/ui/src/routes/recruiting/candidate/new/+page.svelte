<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { CandidateDescriptor } from '../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	const api = createEntityApi(CandidateDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/recruiting/candidate/${item.id}`);
	}

	function handleCancel() {
		goto('/recruiting/candidate');
	}
</script>

<EntityForm
	descriptor={CandidateDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
