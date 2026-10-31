<script lang="ts">
	import { EntityDetail, createEntityApi } from '@crewbase/entities';
	import { PayRunDescriptor } from '../../descriptor';
	import { apiClient } from '#lib/api/client.ts';

	let { data } = $props();

	const api = createEntityApi(PayRunDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={PayRunDescriptor} id={data.id} {api} />
