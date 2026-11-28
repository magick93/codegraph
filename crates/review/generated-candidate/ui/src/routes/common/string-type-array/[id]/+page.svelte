<script lang="ts">
	import { EntityDetail, createEntityApi } from '@app/entities';
	import { StringTypeArrayDescriptor } from '../../descriptor';
	import { apiClient } from '#lib/api/client.ts';

	let { data } = $props();

	const api = createEntityApi(StringTypeArrayDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={StringTypeArrayDescriptor} id={data.id} {api} />
