<script lang="ts">
	import { EntityDetail, createEntityApi } from '@crewbase/entities';
	import { StringTypeArrayDescriptor } from '../../descriptor';
	import { apiClient } from '$lib/api/client';

	let { data } = $props();

	const api = createEntityApi(StringTypeArrayDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={StringTypeArrayDescriptor} id={data.id} {api} />
