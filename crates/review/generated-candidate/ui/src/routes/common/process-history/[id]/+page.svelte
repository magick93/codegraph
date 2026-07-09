<script lang="ts">
	import { EntityDetail, createEntityApi } from '@crewbase/entities';
	import { ProcessHistoryDescriptor } from '../../descriptor';
	import { apiClient } from '$lib/api/client';

	let { data } = $props();

	const api = createEntityApi(ProcessHistoryDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={ProcessHistoryDescriptor} id={data.id} {api} />
