import type { EntityDescriptor } from '@crewbase/entities';

export const ApplicationDescriptor: EntityDescriptor = {
  name: 'Application',
  domain: 'recruiting',
  pathSegment: 'applications',
  operations: ['create', 'read', 'update', 'delete', 'list'],

  fields: [

    {
      name: 'application_id',
      label: 'Application Id',
      type: 'text',
      tsType: 'string',

      required: true,






      list: { visible: true, sortable: true },




    },

    {
      name: 'applied_date',
      label: 'Applied Date',
      type: 'date',
      tsType: 'string',





      validation: {






        format: 'date',

      },


      list: { visible: true },




    },

    {
      name: 'candidate_id_id',
      label: 'Candidate Id Id',
      type: 'text',
      tsType: 'string',






      list: { visible: true },


      ref: { entity: '', domain: 'recruiting', displayField: 'display_name' },



    },

    {
      name: 'status',
      label: 'Status',
      type: 'select',
      tsType: 'string',






      list: { visible: true, badge: true },



      options: {
        source: 'inline',


        values: [

          { value: 'Applied', label: 'Applied' },

          { value: 'Screening', label: 'Screening' },

          { value: 'Interviewing', label: 'Interviewing' },

          { value: 'Offered', label: 'Offered' },

          { value: 'Hired', label: 'Hired' },

          { value: 'Rejected', label: 'Rejected' },

        ],

      },


    },

  ],




};
