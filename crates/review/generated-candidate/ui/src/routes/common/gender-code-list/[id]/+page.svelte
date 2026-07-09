<script lang="ts">
	import { EntityDetail, createEntityApi } from '@crewbase/entities';
	import { GenderCodeListDescriptor } from '../../descriptor';
	import { apiClient } from '$lib/api/client';

	let { data } = $props();

	const api = createEntityApi(GenderCodeListDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={GenderCodeListDescriptor} id={data.id} {api} />
