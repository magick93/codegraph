<script lang="ts">
	import { EntityDetail, createEntityApi } from '@app/entities';
	import { RsvpDescriptor } from '../../descriptor';
	import { apiClient } from '#lib/api/client.ts';

	let { data } = $props();

	const api = createEntityApi(RsvpDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={RsvpDescriptor} id={data.id} {api} />
