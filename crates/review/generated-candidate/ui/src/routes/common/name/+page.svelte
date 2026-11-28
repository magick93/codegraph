<script lang="ts">
	import { EntityList, createEntityApi } from '@app/entities';
	import { NameDescriptor } from './descriptor';
	import { apiClient } from '#lib/api/client.ts';

	const api = createEntityApi(NameDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={NameDescriptor} {api} />
