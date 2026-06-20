<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { CodeDescriptor } from './descriptor';
	import { apiClient } from '$lib/api/client';

	const api = createEntityApi(CodeDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={CodeDescriptor} {api} />
