<script lang="ts">
	import { EntityDetail, createEntityApi } from '@crewbase/entities';
	import { AmountDescriptor } from '../../descriptor';
	import { apiClient } from '$lib/api/client';

	let { data } = $props();

	const api = createEntityApi(AmountDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={AmountDescriptor} id={data.id} {api} />
