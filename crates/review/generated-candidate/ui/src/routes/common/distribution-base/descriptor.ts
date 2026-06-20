import type { EntityDescriptor } from '@crewbase/entities';

export const DistributionBaseDescriptor: EntityDescriptor = {
  name: 'DistributionBase',
  domain: 'common',
  pathSegment: 'distribution-base',
  operations: ['create', 'read', 'update', 'delete', 'list'],

  fields: [

    {
      name: 'description',
      label: 'Description',
      type: 'text',
      tsType: 'string',




      description: 'Distribution description',



      list: { visible: true },




    },

    {
      name: 'end_date',
      label: 'End Date',
      type: 'date',
      tsType: 'string',




      description: 'Distribution end date',


      validation: {






        format: 'date',

      },


      list: { visible: true },




    },

    {
      name: 'start_date',
      label: 'Start Date',
      type: 'date',
      tsType: 'string',

      required: true,




      description: 'Distribution start date',


      validation: {






        format: 'date',

      },


      list: { visible: true, sortable: true },




    },

  ],




};
