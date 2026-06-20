import type { EntityDescriptor } from '@crewbase/entities';

export const IdentifierDescriptor: EntityDescriptor = {
  name: 'Identifier',
  domain: 'common',
  pathSegment: 'identifier',
  operations: ['create', 'read', 'update', 'delete', 'list'],

  fields: [

    {
      name: 'scheme_agency_id',
      label: 'Scheme Agency Id',
      type: 'text',
      tsType: 'string',






      list: { visible: true },




    },

    {
      name: 'scheme_id',
      label: 'Scheme Id',
      type: 'text',
      tsType: 'string',






      list: { visible: true },




    },

    {
      name: 'scheme_version_id',
      label: 'Scheme Version Id',
      type: 'text',
      tsType: 'string',






      list: { visible: true },




    },

    {
      name: 'value',
      label: 'Value',
      type: 'text',
      tsType: 'string',

      required: true,






      list: { visible: true, sortable: true },




    },

  ],




};
