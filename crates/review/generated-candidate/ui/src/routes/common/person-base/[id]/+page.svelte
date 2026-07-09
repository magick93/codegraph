<script lang="ts">
	import { EntityDetail, createEntityApi } from '@crewbase/entities';
	import { PersonBaseDescriptor } from '../../descriptor';
	import { apiClient } from '$lib/api/client';

	let { data } = $props();

	const api = createEntityApi(PersonBaseDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={PersonBaseDescriptor} id={data.id} {api} />
