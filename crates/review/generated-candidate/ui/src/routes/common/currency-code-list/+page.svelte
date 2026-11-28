<script lang="ts">
	import { EntityList, createEntityApi } from '@app/entities';
	import { CurrencyCodeListDescriptor } from './descriptor';
	import { apiClient } from '#lib/api/client.ts';

	const api = createEntityApi(CurrencyCodeListDescriptor, { fetch, baseUrl: apiClient.baseUrl });
</script>

<EntityList descriptor={CurrencyCodeListDescriptor} {api} />
