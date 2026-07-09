<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { StringTypeArrayDescriptor } from './descriptor';
	import { apiClient } from '$lib/api/client';

	const api = createEntityApi(StringTypeArrayDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={StringTypeArrayDescriptor} {api} />
