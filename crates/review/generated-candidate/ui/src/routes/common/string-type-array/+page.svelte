<script lang="ts">
	import { EntityList, createEntityApi } from '@app/entities';
	import { StringTypeArrayDescriptor } from './descriptor';
	import { apiClient } from '#lib/api/client.ts';

	const api = createEntityApi(StringTypeArrayDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={StringTypeArrayDescriptor} {api} />
