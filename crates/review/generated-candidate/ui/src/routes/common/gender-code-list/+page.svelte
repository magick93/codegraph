<script lang="ts">
	import { EntityList, createEntityApi } from '@app/entities';
	import { GenderCodeListDescriptor } from './descriptor';
	import { apiClient } from '#lib/api/client.ts';

	const api = createEntityApi(GenderCodeListDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={GenderCodeListDescriptor} {api} />
