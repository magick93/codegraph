<script lang="ts">
	import { EntityDetail, createEntityApi } from '@app/entities';
	import { NameDescriptor } from '../../descriptor';
	import { apiClient } from '#lib/api/client.ts';

	let { data } = $props();

	const api = createEntityApi(NameDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={NameDescriptor} id={data.id} {api} />
