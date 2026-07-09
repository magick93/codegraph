<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { EffectiveDateDescriptor } from './descriptor';
	import { apiClient } from '$lib/api/client';

	const api = createEntityApi(EffectiveDateDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={EffectiveDateDescriptor} {api} />
