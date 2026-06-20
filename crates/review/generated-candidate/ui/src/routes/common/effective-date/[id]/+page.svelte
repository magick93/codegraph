<script lang="ts">
	import { EntityDetail, createEntityApi } from '@crewbase/entities';
	import { EffectiveDateDescriptor } from '../../descriptor';
	import { apiClient } from '$lib/api/client';

	let { data } = $props();

	const api = createEntityApi(EffectiveDateDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={EffectiveDateDescriptor} id={data.id} {api} />
