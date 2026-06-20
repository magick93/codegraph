<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { PositionScheduleTypeCodeListDescriptor } from '../../../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	let { data } = $props();

	const api = createEntityApi(PositionScheduleTypeCodeListDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess() {
		goto(`/common/position-schedule-type-code-list/${data.id}`);
	}

	function handleCancel() {
		goto(`/common/position-schedule-type-code-list/${data.id}`);
	}
</script>

<EntityForm
	descriptor={PositionScheduleTypeCodeListDescriptor}
	{api}
	mode="edit"
	initialData={data.item}
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
