<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { FormattedDateTimeDescriptor } from './descriptor';
	import { apiClient } from '$lib/api/client';

	const api = createEntityApi(FormattedDateTimeDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={FormattedDateTimeDescriptor} {api} />
