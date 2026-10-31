// Gate-owned stub (entity ui gate): consumer module stand-in.
export const api = {
	get: async <T = unknown>(_path: string): Promise<T> => undefined as T,
	post: async <T = unknown>(_path: string, _body?: unknown): Promise<T> => undefined as T,
	put: async <T = unknown>(_path: string, _body?: unknown): Promise<T> => undefined as T,
	delete: async (_path: string): Promise<void> => undefined,
};
