<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { DistributionBaseDescriptor } from './descriptor';
	import { apiClient } from '$lib/api/client';

	const api = createEntityApi(DistributionBaseDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={DistributionBaseDescriptor} {api} />
