<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { PositionScheduleTypeCodeListDescriptor } from './descriptor';
	import { apiClient } from '$lib/api/client';

	const api = createEntityApi(PositionScheduleTypeCodeListDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={PositionScheduleTypeCodeListDescriptor} {api} />
