<script lang="ts">
	import { EntityList, createEntityApi } from '@crewbase/entities';
	import { CurrencyCodeListDescriptor } from './descriptor';
	import { apiClient } from '$lib/api/client';

	const api = createEntityApi(CurrencyCodeListDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={CurrencyCodeListDescriptor} {api} />
