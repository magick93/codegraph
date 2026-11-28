<script lang="ts">
	import { EntityDetail, createEntityApi } from '@app/entities';
	import { PositionScheduleTypeCodeListDescriptor } from '../../descriptor';
	import { apiClient } from '#lib/api/client.ts';

	let { data } = $props();

	const api = createEntityApi(PositionScheduleTypeCodeListDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={PositionScheduleTypeCodeListDescriptor} id={data.id} {api} />
