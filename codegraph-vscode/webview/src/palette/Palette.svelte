<script lang="ts">
  const items = [
    { type: 'view-container', label: 'View Container', icon: '🔲' },
    { type: 'view-component', label: 'List', icon: '📋', componentType: 'list' },
    { type: 'view-component', label: 'Form', icon: '📝', componentType: 'form' },
    { type: 'view-component', label: 'Details', icon: '📄', componentType: 'details' },
    { type: 'view-component', label: 'Search', icon: '🔍', componentType: 'search' },
    { type: 'view-component', label: 'Tree', icon: '🌳', componentType: 'tree' },
    { type: 'event', label: 'Event', icon: '⚡' },
    { type: 'action', label: 'Action', icon: '🛠️' },
  ];
</script>

<div class="palette">
  <h3>Elements</h3>
  <div class="items">
    {#each items as item}
      <div class="palette-item" draggable="true">
        <span class="icon">{item.icon}</span>
        <span class="label">{item.label}</span>
      </div>
    {/each}
  </div>
</div>

<style>
  .palette {
    padding: 12px;
    border-bottom: 1px solid var(--vscode-panel-border, #ccc);
  }
  .palette h3 {
    margin: 0 0 8px 0;
    font-size: 12px;
    text-transform: uppercase;
    color: var(--vscode-descriptionForeground, #888);
  }
  .palette-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    cursor: grab;
    border-radius: 4px;
    font-size: 13px;
  }
  .palette-item:hover {
    background: var(--vscode-list-hoverBackground, #eee);
  }
  .icon { font-size: 16px; }
</style>
