<script lang="ts">
	import { EntityDetail, createEntityApi } from '@crewbase/entities';
	import { CodeDescriptor } from '../../descriptor';
	import { apiClient } from '$lib/api/client';

	let { data } = $props();

	const api = createEntityApi(CodeDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={CodeDescriptor} id={data.id} {api} />
