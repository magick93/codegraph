<script lang="ts">
	import { EntityList, createEntityApi } from '@app/entities';
	import { CodeDescriptor } from './descriptor';
	import { apiClient } from '#lib/api/client.ts';

	const api = createEntityApi(CodeDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={CodeDescriptor} {api} />
