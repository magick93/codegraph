<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { EventBaseDescriptor } from './descriptor';
	import { apiClient } from '#lib/api/client.ts';

	const api = createEntityApi(EventBaseDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={EventBaseDescriptor} {api} />
