import type { EntityDescriptor } from '@crewbase/entities';

export const PayRunDescriptor: EntityDescriptor = {
  name: 'PayRun',
  domain: 'compensation',
  pathSegment: 'pay-runs',
  operations: ['create', 'read', 'update', 'delete', 'list'],

  fields: [

    {
      name: 'pay_run_id',
      label: 'Pay Run Id',
      type: 'text',
      tsType: 'string',

      required: true,






      list: { visible: true, sortable: true },




    },

    {
      name: 'run_date',
      label: 'Run Date',
      type: 'date',
      tsType: 'string',





      validation: {






        format: 'date',

      },


      list: { visible: true },




    },

    {
      name: 'total_amount',
      label: 'Total Amount',
      type: 'number',
      tsType: 'string',






      list: { visible: true },




    },

    {
      name: 'total_amount_currency',
      label: 'Total Amount Currency',
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

  ],




};
