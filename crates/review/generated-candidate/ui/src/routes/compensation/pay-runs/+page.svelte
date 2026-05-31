<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { PayRunDescriptor } from './descriptor';
	import { apiClient } from '$lib/api/client';

	const api = createEntityApi(PayRunDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={PayRunDescriptor} {api} />
