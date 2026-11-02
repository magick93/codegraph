import type { EntityDescriptor } from '@crewbase/entities';

export const RsvpDescriptor: EntityDescriptor = {
  name: 'Rsvp',
  domain: 'rsvp',
  pathSegment: 'rsvp',
  operations: ['create', 'read', 'update', 'delete', 'list'],

  fields: [

    {
      name: 'event',
      label: 'Event',
      type: 'text',
      tsType: 'RsvpEventBaseResponse',

      required: true,




      description: 'The event this RSVP is for (required value object that resolves to an entity)',



      list: { visible: true, sortable: true },




    },

    {
      name: 'status',
      label: 'Status',
      type: 'select',
      tsType: 'string',

      required: true,






      list: { visible: true, sortable: true, badge: true },



      options: {
        source: 'inline',


        values: [

          { value: 'Confirmed', label: 'Confirmed' },

          { value: 'Pending', label: 'Pending' },

          { value: 'Cancelled', label: 'Cancelled' },

        ],

      },


    },

    {
      name: 'timestamp',
      label: 'Timestamp',
      type: 'datetime-local',
      tsType: 'string',

      required: true,





      validation: {






        format: 'date-time',

      },


      list: { visible: true, sortable: true },




    },

  ],




};
