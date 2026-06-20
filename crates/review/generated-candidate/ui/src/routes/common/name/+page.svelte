<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { NameDescriptor } from './descriptor';
	import { apiClient } from '$lib/api/client';

	const api = createEntityApi(NameDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={NameDescriptor} {api} />
