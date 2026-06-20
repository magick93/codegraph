<script lang="ts">
	import { EntityDetail, createEntityApi } from '@crewbase/entities';
	import { ProcessHistoryItemDescriptor } from '../../descriptor';
	import { apiClient } from '$lib/api/client';

	let { data } = $props();

	const api = createEntityApi(ProcessHistoryItemDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={ProcessHistoryItemDescriptor} id={data.id} {api} />
