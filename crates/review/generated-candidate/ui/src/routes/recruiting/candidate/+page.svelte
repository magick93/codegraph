<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { CandidateDescriptor } from './descriptor';
	import { apiClient } from '$lib/api/client';

	const api = createEntityApi(CandidateDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={CandidateDescriptor} {api} />
