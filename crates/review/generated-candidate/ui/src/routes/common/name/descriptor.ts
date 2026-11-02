import type { EntityDescriptor } from '@crewbase/entities';

export const NameDescriptor: EntityDescriptor = {
  name: 'Name',
  domain: 'common',
  pathSegment: 'name',
  operations: ['create', 'read', 'update', 'delete', 'list'],

  fields: [

    {
      name: 'family_name',
      label: 'Family Name',
      type: 'text',
      tsType: 'string',






      list: { visible: true },




    },

    {
      name: 'formatted_name',
      label: 'Formatted Name',
      type: 'text',
      tsType: 'string',






      list: { visible: true },




    },

    {
      name: 'given_name',
      label: 'Given Name',
      type: 'text',
      tsType: 'string',






      list: { visible: true },




    },

  ],




};
