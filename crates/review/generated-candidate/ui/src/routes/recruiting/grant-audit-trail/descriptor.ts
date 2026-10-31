import type { EntityDescriptor } from '@crewbase/entities';

export const GrantAuditTrailDescriptor: EntityDescriptor = {
  name: 'GrantAuditTrail',
  domain: 'recruiting',
  pathSegment: 'grant-audit-trail',
  operations: ['create', 'read', 'update', 'delete', 'list'],

  fields: [

    {
      name: 'application_id',
      label: 'Application Id',
      type: 'text',
      tsType: 'string',

      required: true,






      list: { visible: true, sortable: true },


      ref: { entity: 'GrantApplicationType', domain: 'recruiting', displayField: 'title' },



    },

  ],




};
