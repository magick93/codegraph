import type { BrowserFilesystemAdapter, BrowserFilesystemDescriptor } from "./filesystem.js";
export interface FileDataEntry {
    dir?: Record<string, FileDataEntry>;
    source?: Uint8Array | string;
    symlink?: string;
}
/**
 * Root file data structure representing a filesystem tree.
 * Each entry is a directory (`dir`), a file (`source`), or a symbolic link (`symlink`).
 * @example
 * // A simple filesystem with one directory containing one file:
 * const fileData = {
 *   dir: {
 *     'myfile.txt': { source: new Uint8Array([72, 101, 108, 108, 111]) }
 *   }
 * };
 */
export type FileData = FileDataEntry;
/**
 * Subscribe to every mutation across every in-memory tree (regardless of which
 * adapter loaded it). Lets a persistence layer (e.g. `OpfsFilesystemAdapter`)
 * react to writes without wrapping every mutating method individually. Returns
 * an unsubscribe function.
 */
export declare function _onTouch(listener: (entry: FileDataEntry) => void): () => void;
/**
 * Well-known symbol a `BrowserFilesystemDescriptor` wrapper (e.g. the cross-tab-locking
 * `Proxy` from `OpfsFilesystemAdapter`) can implement to hand back the real descriptor it
 * wraps. `renameAt`/`linkAt`/`isSameObject` reach into a *second* descriptor argument's
 * private `#entry` field directly - private-field access bypasses `Proxy` traps and fails
 * its brand check against a wrapper, so a wrapper must be unwrapped via ordinary property
 * access (which a `Proxy` handles correctly) before that field access happens.
 */
export declare const UNWRAP_DESCRIPTOR: unique symbol;
/** Explicit ephemeral storage adapter for browser applications and tests. */
export declare class InMemoryFilesystemAdapter implements BrowserFilesystemAdapter<FileData> {
    getRoot(capability: FileData): BrowserFilesystemDescriptor;
}
