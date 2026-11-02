<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { RsvpDescriptor } from './descriptor';
	import { apiClient } from '#lib/api/client.ts';

	const api = createEntityApi(RsvpDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={RsvpDescriptor} {api} />
