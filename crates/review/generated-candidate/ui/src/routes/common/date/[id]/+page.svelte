<script lang="ts">
	import { EntityDetail, createEntityApi } from '@app/entities';
	import { DateDescriptor } from '../../descriptor';
	import { apiClient } from '#lib/api/client.ts';

	let { data } = $props();

	const api = createEntityApi(DateDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={DateDescriptor} id={data.id} {api} />
