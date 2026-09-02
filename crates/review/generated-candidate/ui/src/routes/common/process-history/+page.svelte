<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { ProcessHistoryDescriptor } from './descriptor';
	import { apiClient } from '#lib/api/client.ts';

	const api = createEntityApi(ProcessHistoryDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={ProcessHistoryDescriptor} {api} />
