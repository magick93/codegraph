<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { DateDescriptor } from './descriptor';
	import { apiClient } from '$lib/api/client';

	const api = createEntityApi(DateDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={DateDescriptor} {api} />
