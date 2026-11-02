import type { EntityDescriptor } from '@crewbase/entities';

export const EffectiveDateDescriptor: EntityDescriptor = {
  name: 'EffectiveDate',
  domain: 'common',
  pathSegment: 'effective-date',
  operations: ['create', 'read', 'update', 'delete', 'list'],

  fields: [

    {
      name: 'valid_from',
      label: 'Valid From',
      type: 'date',
      tsType: 'string',





      validation: {






        format: 'date',

      },


      list: { visible: true },




    },

    {
      name: 'valid_to',
      label: 'Valid To',
      type: 'date',
      tsType: 'string',





      validation: {






        format: 'date',

      },


      list: { visible: true },




    },

  ],




};
