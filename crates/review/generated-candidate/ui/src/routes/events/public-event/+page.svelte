<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { PublicEventDescriptor } from './descriptor';
	import { apiClient } from '#lib/api/client.ts';

	const api = createEntityApi(PublicEventDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={PublicEventDescriptor} {api} />
