<script lang="ts">
	import { EntityDetail, createEntityApi } from '@app/entities';
	import { DistributionBaseDescriptor } from '../../descriptor';
	import { apiClient } from '#lib/api/client.ts';

	let { data } = $props();

	const api = createEntityApi(DistributionBaseDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={DistributionBaseDescriptor} id={data.id} {api} />
