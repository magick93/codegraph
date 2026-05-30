import { ChildProcess, spawn } from 'child_process';
import * as vscode from 'vscode';

export class LspServerManager {
    private process: ChildProcess | null = null;

    start(binaryPath: string, args: string[]): ChildProcess {
        this.process = spawn(binaryPath, args, {
            stdio: ['pipe', 'pipe', 'pipe'],
        });

        this.process.stderr?.on('data', (data: Buffer) => {
            console.error(`[codegraph-lsp] ${data.toString()}`);
        });

        this.process.on('exit', (code, signal) => {
            console.log(`[codegraph-lsp] exited code=${code} signal=${signal}`);
            this.process = null;
        });

        this.process.on('error', (err) => {
            console.error(`[codegraph-lsp] spawn error: ${err.message}`);
        });

        return this.process;
    }

    stop(): void {
        if (this.process) {
            this.process.kill('SIGTERM');
            this.process = null;
        }
    }

    isRunning(): boolean {
        return this.process !== null && !this.process.killed;
    }
}
