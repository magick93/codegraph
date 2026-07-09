<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { AmountDescriptor } from './descriptor';
	import { apiClient } from '$lib/api/client';

	const api = createEntityApi(AmountDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={AmountDescriptor} {api} />
