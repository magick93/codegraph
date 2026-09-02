<script lang="ts">
	import { EntityDetail, createEntityApi } from '@crewbase/entities';
	import { PublicEventDescriptor } from '../../descriptor';
	import { apiClient } from '#lib/api/client.ts';

	let { data } = $props();

	const api = createEntityApi(PublicEventDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={PublicEventDescriptor} id={data.id} {api} />
