import type { EntityDescriptor } from '@app/entities';

export const CodeDescriptor: EntityDescriptor = {
  name: 'Code',
  domain: 'common',
  pathSegment: 'code',
  operations: ['create', 'read', 'update', 'delete', 'list'],

  fields: [

  ],




};
