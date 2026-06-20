<script lang="ts">
	import { EntityDetail, createEntityApi } from '@crewbase/entities';
	import { CurrencyCodeListDescriptor } from '../../descriptor';
	import { apiClient } from '$lib/api/client';

	let { data } = $props();

	const api = createEntityApi(CurrencyCodeListDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityDetail descriptor={CurrencyCodeListDescriptor} id={data.id} {api} />
