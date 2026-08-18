import type { EntityDescriptor } from '@crewbase/entities';

export const PublicEventDescriptor: EntityDescriptor = {
  name: 'PublicEvent',
  domain: 'events',
  pathSegment: 'public-event',
  operations: ['create', 'read', 'update', 'delete', 'list'],

  fields: [

    {
      name: 'is_published',
      label: 'Is Published',
      type: 'checkbox',
      tsType: 'boolean',






      list: { visible: true },




    },

    {
      name: 'capacity',
      label: 'Capacity',
      type: 'number',
      tsType: 'number',






      list: { visible: true },




    },

    {
      name: 'title',
      label: 'Title',
      type: 'text',
      tsType: 'string',

      required: true,




      description: 'Event title',



      list: { visible: true, sortable: true },




    },

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
