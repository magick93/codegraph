<script lang="ts">
	import { EntityDetail, createEntityApi } from '@crewbase/entities';
	import { FormattedDateTimeDescriptor } from '../../descriptor';
	import { apiClient } from '$lib/api/client';

	let { data } = $props();

	const api = createEntityApi(FormattedDateTimeDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={FormattedDateTimeDescriptor} id={data.id} {api} />
