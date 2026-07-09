<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { GenderCodeListDescriptor } from './descriptor';
	import { apiClient } from '$lib/api/client';

	const api = createEntityApi(GenderCodeListDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={GenderCodeListDescriptor} {api} />
