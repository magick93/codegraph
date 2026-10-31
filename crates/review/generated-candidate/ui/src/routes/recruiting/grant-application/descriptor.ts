import type { EntityDescriptor } from '@crewbase/entities';

export const GrantApplicationDescriptor: EntityDescriptor = {
  name: 'GrantApplication',
  domain: 'recruiting',
  pathSegment: 'grant-application',
  operations: ['create', 'read', 'update', 'delete', 'list'],

  fields: [

    {
      name: 'amount',
      label: 'Amount',
      type: 'number',
      tsType: 'number',

      required: true,






      list: { visible: true, sortable: true },




    },

    {
      name: 'approved',
      label: 'Approved',
      type: 'checkbox',
      tsType: 'boolean',






      list: { visible: true },




    },

    {
      name: 'category',
      label: 'Category',
      type: 'text',
      tsType: 'string',






      list: { visible: true },




    },

    {
      name: 'id',
      label: 'Id',
      type: 'text',
      tsType: 'string',

      required: true,






      list: { visible: true, sortable: true },




    },

    {
      name: 'lodged_on',
      label: 'Lodged On',
      type: 'datetime-local',
      tsType: 'string',





      validation: {






        format: 'date-time',

      },


      list: { visible: true },




    },

    {
      name: 'notes',
      label: 'Notes',
      type: 'text',
      tsType: 'string',






      list: { visible: true },




    },

    {
      name: 'reference',
      label: 'Reference',
      type: 'text',
      tsType: 'string',

      required: true,






      list: { visible: true, sortable: true },




    },

    {
      name: 'summary',
      label: 'Summary',
      type: 'text',
      tsType: 'string',






      list: { visible: true },




    },

    {
      name: 'title',
      label: 'Title',
      type: 'text',
      tsType: 'string',






      list: { visible: true },




    },

  ],




};
