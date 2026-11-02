import type { EntityDescriptor } from '@crewbase/entities';

export const PersonBaseDescriptor: EntityDescriptor = {
  name: 'PersonBase',
  domain: 'common',
  pathSegment: 'person-base',
  operations: ['create', 'read', 'update', 'delete', 'list'],

  fields: [

    {
      name: 'birth_date',
      label: 'Birth Date',
      type: 'date',
      tsType: 'string',





      validation: {






        format: 'date',

      },


      list: { visible: true },




    },

    {
      name: 'family_name',
      label: 'Family Name',
      type: 'text',
      tsType: 'string',

      required: true,




      description: 'Last name',



      list: { visible: true, sortable: true },




    },

    {
      name: 'given_name',
      label: 'Given Name',
      type: 'text',
      tsType: 'string',

      required: true,




      description: 'First name',



      list: { visible: true, sortable: true },




    },

  ],




};
