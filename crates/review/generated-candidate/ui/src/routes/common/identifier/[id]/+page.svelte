<script lang="ts">
	import { EntityDetail, createEntityApi } from '@app/entities';
	import { IdentifierDescriptor } from '../../descriptor';
	import { apiClient } from '#lib/api/client.ts';

	let { data } = $props();

	const api = createEntityApi(IdentifierDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={IdentifierDescriptor} id={data.id} {api} />
