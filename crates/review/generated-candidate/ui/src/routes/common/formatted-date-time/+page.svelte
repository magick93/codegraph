<script lang="ts">
	import { EntityList, createEntityApi } from '@app/entities';
	import { FormattedDateTimeDescriptor } from './descriptor';
	import { apiClient } from '#lib/api/client.ts';

	const api = createEntityApi(FormattedDateTimeDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={FormattedDateTimeDescriptor} {api} />
