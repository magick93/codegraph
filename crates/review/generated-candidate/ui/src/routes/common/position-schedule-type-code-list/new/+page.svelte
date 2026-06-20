<script lang="ts">
	import { EntityForm, createEntityApi } from '@crewbase/entities';
	import { PositionScheduleTypeCodeListDescriptor } from '../descriptor';
	import { apiClient } from '$lib/api/client';
	import { goto } from '$app/navigation';

	const api = createEntityApi(PositionScheduleTypeCodeListDescriptor, { fetch, baseUrl: apiClient.baseUrl });

	function handleSuccess(item: Record<string, unknown>) {
		goto(`/common/position-schedule-type-code-list/${item.id}`);
	}

	function handleCancel() {
		goto('/common/position-schedule-type-code-list');
	}
</script>

<EntityForm
	descriptor={PositionScheduleTypeCodeListDescriptor}
	{api}
	mode="create"
	onSuccess={handleSuccess}
	onCancel={handleCancel}
/>
