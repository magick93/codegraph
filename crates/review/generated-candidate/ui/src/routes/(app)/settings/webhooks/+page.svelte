
<script lang="ts">
  import { invalidateAll } from '$app/navigation';
  import * as Card from '#lib/components/ui/card/index.js';
  import { Badge } from '#lib/components/ui/badge/index.js';
  import { Button } from '#lib/components/ui/button/index.js';
  import { deleteEndpoint } from './data.remote';

  interface WebhookEndpoint {
    id: string;
    url: string;
    description: string | null;
    is_active: boolean;
    created_at: string;
    updated_at: string;
  }

  interface Props {
    data: { endpoints: WebhookEndpoint[] };
  }

  let { data }: Props = $props();
  let endpoints = $derived(data.endpoints);
  let deletingId = $state<string | null>(null);

  async function handleDelete(id: string) {
    if (!confirm('Delete this webhook endpoint? This cannot be undone.')) return;
    deletingId = id;
    try {
      const result = await deleteEndpoint(id);
      if (!result.ok) {
        throw new Error(result.message ?? 'Request failed');
      }
      await invalidateAll();
    } finally {
      deletingId = null;
    }
  }
</script>


<div class="space-y-6 p-6">
  <div class="flex items-center justify-between">
    <div>
      <h2 class="text-2xl font-bold tracking-tight">Webhooks</h2>
      <p class="text-muted-foreground">
        Send real-time event notifications to external URLs.
      </p>
    </div>
    <Button href="/settings/webhooks/new">Add Endpoint</Button>
  </div>

  {#if endpoints.length === 0}
    <div class="py-12 text-center">
      <p class="text-muted-foreground">No webhook endpoints configured yet.</p>
      <p class="text-sm text-muted-foreground mt-1">Create one to start receiving event notifications.</p>
    </div>
  {:else}
    <div class="grid gap-4">
      {#each endpoints as endpoint}
        <Card.Root>
          <Card.Header class="flex flex-row items-center gap-4">
            <div class="flex-1">
              <div class="flex items-center gap-2">
                <Card.Title class="text-base font-mono text-sm">{endpoint.url}</Card.Title>
                {#if endpoint.is_active}
                  <Badge variant="default" class="bg-green-100 text-green-800">Active</Badge>
                {:else}
                  <Badge variant="outline" class="text-muted-foreground">Disabled</Badge>
                {/if}
              </div>
              {#if endpoint.description}
                <Card.Description>{endpoint.description}</Card.Description>
              {/if}
            </div>
            <div class="flex gap-2">
              <Button variant="outline" size="sm" href="/settings/webhooks/{endpoint.id}">
                View
              </Button>
              <Button variant="outline" size="sm" href="/settings/webhooks/{endpoint.id}/edit">
                Edit
              </Button>
              <Button variant="destructive" size="sm" disabled={deletingId === endpoint.id} onclick={() => handleDelete(endpoint.id)}>
                Delete
              </Button>
            </div>
          </Card.Header>
        </Card.Root>
      {/each}
    </div>
  {/if}

</div>
