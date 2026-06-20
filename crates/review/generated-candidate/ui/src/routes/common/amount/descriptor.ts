import type { EntityDescriptor } from '@crewbase/entities';

export const AmountDescriptor: EntityDescriptor = {
  name: 'Amount',
  domain: 'common',
  pathSegment: 'amount',
  operations: ['create', 'read', 'update', 'delete', 'list'],

  fields: [

    {
      name: 'currency',
      label: 'Currency',
      type: 'select',
      tsType: 'string',






      list: { visible: true, badge: true },



      options: {
        source: 'inline',


        values: [

          { value: 'USD', label: 'USD' },

          { value: 'EUR', label: 'EUR' },

          { value: 'GBP', label: 'GBP' },

          { value: 'JPY', label: 'JPY' },

          { value: 'AUD', label: 'AUD' },

        ],

      },


    },

    {
      name: 'value',
      label: 'Value',
      type: 'number',
      tsType: 'number',






      list: { visible: true },




    },

  ],




};
