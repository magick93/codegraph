<script lang="ts">
	import { EntityDetail, createEntityApi } from '@crewbase/entities';
	import { ApplicationDescriptor } from '../../descriptor';
	import { apiClient } from '$lib/api/client';

	let { data } = $props();

	const api = createEntityApi(ApplicationDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={ApplicationDescriptor} id={data.id} {api} />
