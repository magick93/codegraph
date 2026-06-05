<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { ApplicationDescriptor } from './descriptor';
	import { apiClient } from '$lib/api/client';

	const api = createEntityApi(ApplicationDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={ApplicationDescriptor} {api} />
