
<script lang="ts">
  import { invalidateAll } from '$app/navigation';
  import * as Card from '#lib/components/ui/card/index.js';
  import { Badge } from '#lib/components/ui/badge/index.js';
  import { Button } from '#lib/components/ui/button/index.js';
  import { rotateSecret, toggleActive, deleteSubscription } from './data.remote';

  interface WebhookEndpoint {
    id: string;
    url: string;
    description: string | null;
    headers: Record<string, string>;
    is_active: boolean;
    created_at: string;
    updated_at: string;
  }

  interface Subscription {
    id: string;
    endpoint_id: string;
    event_domain: string;
    event_entity: string | null;
    event_type: string | null;
    filter: Record<string, unknown>;
    is_active: boolean;
    created_at: string;
  }

  interface Delivery {
    id: string;
    endpoint_id: string;
    subscription_id: string;
    response_status: number | null;
    attempt: number;
    delivered_at: string | null;
    next_retry_at: string | null;
    created_at: string;
  }

  interface Props {
    data: {
      endpoint: WebhookEndpoint;
      subscriptions: Subscription[];
      deliveries: Delivery[];
    };
  }

  let { data }: Props = $props();
  let endpoint = $derived(data.endpoint);
  let subscriptions = $derived(data.subscriptions);
  let deliveries = $derived(data.deliveries);
  let error = $state<string | null>(null);

  const endpointId = $derived(endpoint.id);

  async function handleToggleActive() {
    error = null;
    try {
      const result = await toggleActive(endpointId, !endpoint.is_active);
      if (!result.ok) {
        throw new Error(result.message ?? 'Request failed');
      }
      await invalidateAll();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to toggle status';
    }
  }

  async function handleRotateSecret() {
    if (!confirm('Rotate the HMAC secret? Existing integrations using the old secret will stop working.')) return;
    error = null;
    try {
      const result = await rotateSecret(endpointId);
      if (!result.ok) {
        throw new Error(result.message ?? 'Request failed');
      }
      alert(`New secret: ${result.secret}\n\nCopy this now — it will not be shown again.`);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to rotate secret';
    }
  }

  async function handleDeleteSubscription(id: string) {
    if (!confirm('Remove this subscription?')) return;
    error = null;
    try {
      const result = await deleteSubscription(id);
      if (!result.ok) {
        throw new Error(result.message ?? 'Request failed');
      }
      await invalidateAll();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to delete subscription';
    }
  }

  function statusBadge(delivery: Delivery) {
    if (delivery.delivered_at) return { label: 'Delivered', variant: 'default', class: 'bg-green-100 text-green-800' };
    if (delivery.attempt > 0) return { label: `Failed (attempt ${delivery.attempt})`, variant: 'destructive', class: '' };
    return { label: 'Pending', variant: 'outline', class: 'text-muted-foreground' };
  }
</script>


<div class="space-y-6 p-6">
  {#if error}
    <div class="rounded-md bg-destructive/10 p-3 text-sm text-destructive">{error}</div>
  {/if}

  <div class="flex items-center justify-between">
    <div>
      <div class="flex items-center gap-2">
        <h2 class="text-2xl font-bold tracking-tight font-mono text-sm">{endpoint.url}</h2>
        {#if endpoint.is_active}
          <Badge variant="default" class="bg-green-100 text-green-800">Active</Badge>
        {:else}
          <Badge variant="outline" class="text-muted-foreground">Disabled</Badge>
        {/if}
      </div>
      {#if endpoint.description}
        <p class="text-muted-foreground">{endpoint.description}</p>
      {/if}
    </div>
    <div class="flex gap-2">
      <Button variant="outline" onclick={handleToggleActive}>
        {endpoint.is_active ? 'Disable' : 'Enable'}
      </Button>
      <Button variant="outline" href="/settings/webhooks/{endpointId}/edit">Edit</Button>
      <Button variant="outline" onclick={handleRotateSecret}>Rotate Secret</Button>
    </div>
  </div>

  <!-- Subscriptions -->
  <Card.Root>
    <Card.Header>
      <Card.Title>Event Subscriptions</Card.Title>
      <Card.Description>Subscribe this endpoint to domain events.</Card.Description>
    </Card.Header>
    <Card.Content>
      {#if subscriptions.length === 0}
        <p class="text-sm text-muted-foreground py-4 text-center">No subscriptions configured.</p>
      {:else}
        <div class="space-y-2">
          {#each subscriptions as sub}
            <div class="flex items-center justify-between rounded-md border p-3">
              <div>
                <span class="font-mono text-sm">{sub.event_domain}.{sub.event_entity ?? '*'}.{sub.event_type ?? '*'}</span>
              </div>
              <Button variant="ghost" size="sm" onclick={() => handleDeleteSubscription(sub.id)}>Remove</Button>
            </div>
          {/each}
        </div>
      {/if}
    </Card.Content>
  </Card.Root>

  <!-- Delivery Log -->
  <Card.Root>
    <Card.Header>
      <Card.Title>Delivery Log</Card.Title>
      <Card.Description>Recent webhook delivery attempts.</Card.Description>
    </Card.Header>
    <Card.Content>
      {#if deliveries.length === 0}
        <p class="text-sm text-muted-foreground py-4 text-center">No deliveries yet.</p>
      {:else}
        <div class="space-y-2">
          {#each deliveries as delivery}
            <div class="flex items-center justify-between rounded-md border p-3 text-sm">
              <div class="flex items-center gap-3">
                <Badge variant="default" class={statusBadge(delivery).class}>
                  {statusBadge(delivery).label}
                </Badge>
                <span class="text-muted-foreground">Attempt {delivery.attempt}</span>
                <span class="text-muted-foreground">{new Date(delivery.created_at).toLocaleString()}</span>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </Card.Content>
  </Card.Root>
</div>

