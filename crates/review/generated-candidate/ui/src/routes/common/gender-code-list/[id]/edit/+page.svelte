<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { GenderCodeListDescriptor } from '../../../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(GenderCodeListDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/common/gender-code-list/${data.id}`);
	}

	function handleCancel() {
		goto(`/common/gender-code-list/${data.id}`);
	}
</script>

<EntityForm
	descriptor={GenderCodeListDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
