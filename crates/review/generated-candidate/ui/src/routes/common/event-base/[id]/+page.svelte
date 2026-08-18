<script lang="ts">
	import { EntityDetail, createEntityApi } from '@crewbase/entities';
	import { EventBaseDescriptor } from '../../descriptor';
	import { apiClient } from '$lib/api/client';

	let { data } = $props();

	const api = createEntityApi(EventBaseDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={EventBaseDescriptor} id={data.id} {api} />
