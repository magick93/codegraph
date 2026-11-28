<script lang="ts">
	import { EntityList, createEntityApi } from '@app/entities';
	import { PersonBaseDescriptor } from './descriptor';
	import { apiClient } from '#lib/api/client.ts';

	const api = createEntityApi(PersonBaseDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={PersonBaseDescriptor} {api} />
