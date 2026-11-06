// Gate-owned stub: the consumer monorepo owns the real package.
// Runtime exports (the bundler resolves value imports against these).
export class ApiError extends Error {}
export interface EntityDescriptor {
	entity: string;
	label: string;
	fields: { name: string; label: string; type?: string; tsType?: string }[];
	[key: string]: unknown;
}
export interface CodelistData {
	[key: string]: unknown;
}
export function createEntityApi(_descriptor: EntityDescriptor, _opts?: unknown) {
	return {
		list: async () => [],
		get: async (_id: string) => ({}),
		create: async (_data: unknown) => ({}),
		update: async (_id: string, _data: unknown) => ({}),
		remove: async (_id: string) => undefined,
	};
}
export function TimeAgo(_props: { value?: string }) {}
export function EntityList(_props: Record<string, unknown>) {}
export function EntityDetail(_props: Record<string, unknown>) {}
export function EntityForm(_props: Record<string, unknown>) {}
export function formatValue(_value: unknown): string {
	return '';
}
