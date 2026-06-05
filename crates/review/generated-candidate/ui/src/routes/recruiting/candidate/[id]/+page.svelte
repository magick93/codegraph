<script lang="ts">
	import { EntityDetail, createEntityApi } from '@crewbase/entities';
	import { CandidateDescriptor } from '../../descriptor';
	import { apiClient } from '$lib/api/client';

	let { data } = $props();

	const api = createEntityApi(CandidateDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={CandidateDescriptor} id={data.id} {api} />
