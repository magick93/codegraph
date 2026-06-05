import type { IfmlModel, SyncMessage, CodegenConfig } from './types';

export class SyncClient {
  private vscode: any;

  constructor() {
    // @ts-ignore
    this.vscode = acquireVsCodeApi?.();
  }

  postMessage(msg: SyncMessage): void {
    if (this.vscode) {
      this.vscode.postMessage(msg);
    }
  }

  onMessage(handler: (msg: SyncMessage) => void): void {
    window.addEventListener('message', (event: MessageEvent) => {
      handler(event.data as SyncMessage);
    });
  }

  sendDiagramChange(model: IfmlModel): void {
    this.postMessage({ command: 'sync/diagramChanged', model });
  }

  sendCodegenToggle(framework: string, enabled: boolean) {
    this.postMessage({ command: 'sync/codegenToggle', framework, enabled } as any);
  }

  sendCodegenRun() {
    this.postMessage({ command: 'sync/codegenRun' } as any);
  }
}
