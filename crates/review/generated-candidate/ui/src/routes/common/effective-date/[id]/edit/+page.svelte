<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { EffectiveDateDescriptor } from '../../../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(EffectiveDateDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/common/effective-date/${data.id}`);
	}

	function handleCancel() {
		goto(`/common/effective-date/${data.id}`);
	}
</script>

<EntityForm
	descriptor={EffectiveDateDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
