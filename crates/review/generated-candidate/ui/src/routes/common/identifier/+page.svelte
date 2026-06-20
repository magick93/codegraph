<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { IdentifierDescriptor } from './descriptor';
	import { apiClient } from '$lib/api/client';

	const api = createEntityApi(IdentifierDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={IdentifierDescriptor} {api} />
