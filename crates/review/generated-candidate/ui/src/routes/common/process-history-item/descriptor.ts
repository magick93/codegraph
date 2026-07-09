import type { EntityDescriptor } from '@crewbase/entities';

export const ProcessHistoryItemDescriptor: EntityDescriptor = {
  name: 'ProcessHistoryItem',
  domain: 'common',
  pathSegment: 'process-history-item',
  operations: ['create', 'read', 'update', 'delete', 'list'],

  fields: [

    {
      name: 'action_date',
      label: 'Action Date',
      type: 'datetime-local',
      tsType: 'string',




      description: 'The date the action was executed.',



      list: { visible: true },




    },

    {
      name: 'descriptions',
      label: 'Descriptions',
      type: 'array',
      tsType: 'Array<string>',




      description: 'Additional details about the history item.',



      list: { visible: true },




    },

    {
      name: 'id',
      label: 'Id',
      type: 'text',
      tsType: 'string',




      description: 'The identifier for the history item.',



      list: { visible: true },




    },

  ],




};
