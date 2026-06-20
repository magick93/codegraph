<script lang="ts">
	import { EntityDetail, createEntityApi } from '@crewbase/entities';
	import { PositionScheduleTypeCodeListDescriptor } from '../../descriptor';
	import { apiClient } from '$lib/api/client';

	let { data } = $props();

	const api = createEntityApi(PositionScheduleTypeCodeListDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={PositionScheduleTypeCodeListDescriptor} id={data.id} {api} />
