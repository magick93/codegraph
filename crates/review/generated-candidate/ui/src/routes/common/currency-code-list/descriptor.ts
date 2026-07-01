import type { EntityDescriptor } from '@crewbase/entities';

export const CurrencyCodeListDescriptor: EntityDescriptor = {
  name: 'CurrencyCodeList',
  domain: 'common',
  pathSegment: 'currency-code-list',
  operations: ['create', 'read', 'update', 'delete', 'list'],

  fields: [

    {
      name: 'code',
      label: 'Code',
      type: 'code',
      tsType: 'string',

      required: true,






      list: { visible: true, sortable: true },




    },

    {
      name: 'display_name',
      label: 'Display Name',
      type: 'text',
      tsType: 'string',

      required: true,






      list: { visible: true, sortable: true },




    },

    {
      name: 'sort_order',
      label: 'Sort Order',
      type: 'number',
      tsType: 'string',






      list: { visible: true },




    },

  ],




};
