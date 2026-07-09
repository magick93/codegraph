<script lang="ts">
	import { EntityDetail, createEntityApi } from '@crewbase/entities';
	import { NameDescriptor } from '../../descriptor';
	import { apiClient } from '$lib/api/client';

	let { data } = $props();

	const api = createEntityApi(NameDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={NameDescriptor} id={data.id} {api} />
