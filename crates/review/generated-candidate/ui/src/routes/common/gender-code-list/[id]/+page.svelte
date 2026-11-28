<script lang="ts">
	import { EntityDetail, createEntityApi } from '@app/entities';
	import { GenderCodeListDescriptor } from '../../descriptor';
	import { apiClient } from '#lib/api/client.ts';

	let { data } = $props();

	const api = createEntityApi(GenderCodeListDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={GenderCodeListDescriptor} id={data.id} {api} />
