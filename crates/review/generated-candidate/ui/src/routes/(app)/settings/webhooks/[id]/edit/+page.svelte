
<script lang="ts">
  import { page } from '$app/state';
  import { goto } from '$app/navigation';
  import { Button } from '#lib/components/ui/button/index.js';
  import { Input } from '#lib/components/ui/input/index.js';
  import { Label } from '#lib/components/ui/label/index.js';
  import { Textarea } from '#lib/components/ui/textarea/index.js';
  import { createEndpoint, updateEndpoint } from './data.remote';

  interface WebhookEndpoint {
    id: string;
    url: string;
    description: string | null;
    headers: Record<string, string>;
  }

  const isEdit = $derived(!!page.params.id);
  const endpointId = $derived(page.params.id);
  // The edit route's load provides the endpoint; the new route carries no
  // load, so the prop is read defensively (issue #574 S2).
  const existing = $derived(
    (data as { endpoint?: WebhookEndpoint } | undefined)?.endpoint ?? null
  );

  let url = $state(existing?.url ?? '');
  let description = $state(existing?.description ?? '');
  let headersJson = $state(existing ? JSON.stringify(existing.headers ?? {}, null, 2) : '{}');
  let saving = $state(false);
  let error = $state<string | null>(null);

  async function handleSubmit(e: Event) {
    e.preventDefault();
    saving = true;
    error = null;
    try {
      let headers: Record<string, string> = {};
      try {
        headers = JSON.parse(headersJson);
      } catch {
        throw new Error('Invalid JSON in headers');
      }

      const body = { url, description: description || null, headers };
      const result = isEdit
        ? await updateEndpoint(endpointId, body)
        : await createEndpoint(body);

      if (!result.ok) {
        throw new Error(result.message ?? 'Request failed');
      }

      await goto(`/settings/webhooks/${result.id}`);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to save';
    } finally {
      saving = false;
    }
  }
</script>


<div class="space-y-6 p-6 max-w-2xl">
  <div>
    <h2 class="text-2xl font-bold tracking-tight">{isEdit ? 'Edit Webhook' : 'New Webhook'}</h2>
    <p class="text-muted-foreground">
      {isEdit ? 'Update the webhook endpoint configuration.' : 'Create a new webhook endpoint to receive event notifications.'}
    </p>
  </div>

  {#if error}
    <div class="rounded-md bg-destructive/10 p-3 text-sm text-destructive">{error}</div>
  {/if}

  <form class="space-y-4">
    <div class="space-y-2">
      <Label for="url">URL</Label>
      <Input id="url" type="url" bind:value={url} placeholder="https://example.com/webhooks" required />
    </div>

    <div class="space-y-2">
      <Label for="description">Description (optional)</Label>
      <Input id="description" bind:value={description} placeholder="My webhook endpoint" />
    </div>

    <div class="space-y-2">
      <Label for="headers">Custom Headers (JSON, optional)</Label>
      <Textarea id="headers" bind:value={headersJson} rows={4} class="font-mono text-xs" />
    </div>

    <div class="flex gap-3">
      <Button disabled={!url || saving} onclick={handleSubmit} data-testid="webhook-endpoint-submit-btn">
        {saving ? 'Saving...' : isEdit ? 'Update' : 'Create'}
      </Button>
      <Button variant="outline" href="/settings/webhooks">Cancel</Button>
    </div>
  </form>
</div>

