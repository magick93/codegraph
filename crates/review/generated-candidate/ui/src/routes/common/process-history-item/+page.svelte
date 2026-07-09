<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { ProcessHistoryItemDescriptor } from './descriptor';
	import { apiClient } from '$lib/api/client';

	const api = createEntityApi(ProcessHistoryItemDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={ProcessHistoryItemDescriptor} {api} />
